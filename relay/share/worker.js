// quickshort-share — image share-link relay.
//
// POST   /u            -> { url, delete_key, expires_in } — stores the
//                         image in R2 and a small meta:<id> record in
//                         KV. Body is image/webp or image/png.
// GET    /s/<id>       -> static landing page: image centered, house
//                         ad, optional ad-network slot (ADS_ENABLED).
// GET    /s/<id>.<ext> -> raw image bytes (ext = webp|png). This is
//                         what og:image and chat unfurlers fetch.
// DELETE /s/<id>.<ext> -> revoke early; needs the X-Delete-Key header
//                         that only the uploader received. KV stores
//                         SHA-256(key), never the key itself.
//
// Retention: a daily cron drops shares nobody requested in
// RETENTION_S (30 d). Reads refresh meta.last_access throttled to
// once per ACCESS_FLUSH_S so hot links stay alive without burning
// the KV-write quota. An R2 bucket lifecycle rule (90 d) is the hard
// backstop for orphaned blobs; meta TTL mirrors it.
//
// Abuse controls: 12 MB cap, 30 uploads/IP/day. The client binary is
// public so no real auth is possible; unguessable 128-bit ids keep
// shared files private by obscurity.
const MAX_BYTES = 12 * 1024 * 1024;
const RETENTION_S = 30 * 24 * 3600;
const META_TTL_S = 90 * 24 * 3600;
const ACCESS_FLUSH_S = 3600;
const RATE_PER_DAY = 30;
const TYPES = { "image/webp": "webp", "image/png": "png" };

// Display-ad slot — hidden until a network is approved. Paste the
// snippet below and flip the flag; CSP already whitelists AdSense.
const ADS_ENABLED = false;
const AD_SNIPPET = "";

export default {
  async fetch(req, env, ctx) {
    const url = new URL(req.url);
    if (req.method === "POST" && url.pathname === "/u") return upload(req, env, ctx, url);
    const raw = parseRawPath(url);
    if (raw) {
      if (req.method === "GET") return serveRaw(raw, env, ctx);
      if (req.method === "DELETE") return revoke(req, raw, env, ctx);
    }
    if (req.method === "GET" && parsePagePath(url)) {
      return servePage(url, env, ctx);
    }
    if (url.pathname === "/healthz") return new Response("ok");
    return new Response("not found", { status: 404 });
  },

  // Daily cron: delete shares nobody has requested in RETENTION_S,
  // blob and meta together. KV list is cursor-paginated.
  async scheduled(_event, env, ctx) {
    const cutoff = Date.now() / 1000 - RETENTION_S;
    let cursor;
    do {
      const page = await env.SHARE.list({ prefix: "meta:", cursor, limit: 512 });
      for (const k of page.keys) {
        ctx.waitUntil(
          env.SHARE.get(k.name)
            .then((v) => (v ? JSON.parse(v) : null))
            .then(async (m) => {
              if (!m || (m.last_access ?? m.created ?? 0) > cutoff) return;
              await Promise.all([
                env.BLOB.delete(`shares/${k.name.slice(5)}.${m.ext}`),
                env.SHARE.delete(k.name),
              ]);
            })
            .catch(() => {}),
        );
      }
      cursor = page.list_complete ? undefined : page.cursor;
    } while (cursor);
  },
};

// /s/<32-hex>.<webp|png> — the raw image route
function parseRawPath(url) {
  const m = /^\/s\/([a-f0-9]{32})\.(webp|png)$/.exec(url.pathname);
  return m && { id: m[1], ext: m[2] };
}

// /s/<32-hex> — the landing-page route
function parsePagePath(url) {
  return /^\/s\/[a-f0-9]{32}$/.test(url.pathname);
}

const metaKey = (id) => `meta:${id}`;
const blobKey = (id, ext) => `shares/${id}.${ext}`;

async function sha256hex(text) {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(text)
  );
  return [...new Uint8Array(digest)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function upload(req, env, ctx, url) {
  const ct = (req.headers.get("content-type") || "").split(";")[0].trim();
  const ext = TYPES[ct];
  if (!ext) return err(415, "image/webp or image/png only");
  if (Number(req.headers.get("content-length") || 0) > MAX_BYTES) {
    return err(413, "too large");
  }
  const ip = req.headers.get("cf-connecting-ip") || "unknown";
  const rk = `rl:${ip}:${new Date().toISOString().slice(0, 10)}`;
  const n = Number((await env.SHARE.get(rk)) || 0);
  if (n >= RATE_PER_DAY) return err(429, "daily share limit reached");

  const body = await req.arrayBuffer();
  if (!body.byteLength || body.byteLength > MAX_BYTES) return err(413, "bad size");
  const id = crypto.randomUUID().replaceAll("-", "");
  const deleteKey = crypto.randomUUID().replaceAll("-", "");
  const now = Math.floor(Date.now() / 1000);

  await env.BLOB.put(blobKey(id, ext), body, {
    httpMetadata: { contentType: ct },
  });
  await env.SHARE.put(
    metaKey(id),
    JSON.stringify({
      ext,
      dk: await sha256hex(deleteKey),
      created: now,
      last_access: now,
      hits: 0,
    }),
    { expirationTtl: META_TTL_S },
  );
  ctx.waitUntil(env.SHARE.put(rk, String(n + 1), { expirationTtl: 2 * 86400 }));
  return Response.json({
    url: `${url.origin}/s/${id}`,
    delete_key: deleteKey,
    expires_in: RETENTION_S,
  });
}

/// Bump meta.last_access/hits — throttled to one KV write per
/// ACCESS_FLUSH_S per object, always off the response path.
function touch(env, ctx, id, meta) {
  const now = Math.floor(Date.now() / 1000);
  if (now - (meta.last_access ?? 0) < ACCESS_FLUSH_S) return;
  meta.last_access = now;
  meta.hits = (meta.hits ?? 0) + 1;
  ctx.waitUntil(env.SHARE.put(metaKey(id), JSON.stringify(meta), { expirationTtl: META_TTL_S }));
}

/// Raw bytes for og:image, embeds and the download button.
async function serveRaw({ id, ext }, env, ctx) {
  const obj = await env.BLOB.get(blobKey(id, ext));
  if (!obj) return new Response("gone", { status: 404 });
  const meta = await env.SHARE.get(metaKey(id)).then((v) => v && JSON.parse(v));
  if (meta) touch(env, ctx, id, meta);
  return new Response(obj.body, {
    headers: {
      "content-type": `image/${ext}`,
      "cache-control": "public, max-age=86400",
      "x-content-type-options": "nosniff",
    },
  });
}

/// The landing page — static shell around the raw image URL. CSP
/// keeps uploaded content out of the page's script context; the ad
/// slot is inert until ADS_ENABLED.
async function servePage(url, env, ctx) {
  const id = url.pathname.slice(3);
  const meta = await env.SHARE.get(metaKey(id)).then((v) => v && JSON.parse(v));
  if (!meta) return new Response("gone", { status: 404 });
  touch(env, ctx, id, meta);
  return page(url, id, meta.ext);
}

function page(url, id, ext) {
  const raw = `${url.origin}/s/${id}.${ext}`;
  const html = `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Quickshort capture</title>
<meta property="og:type" content="website">
<meta property="og:title" content="Quickshort capture">
<meta property="og:image" content="${raw}">
<meta name="twitter:card" content="summary_large_image">
<style>
  *{box-sizing:border-box;margin:0}
  body{min-height:100vh;display:flex;flex-direction:column;background:#0d0f13;color:#d6dae1;font:14px/1.5 system-ui,sans-serif}
  header{display:flex;align-items:center;gap:10px;padding:14px 22px;border-bottom:1px solid #22262e}
  header b{font-size:15px;letter-spacing:.2px}
  header .tag{font-size:12px;color:#8b93a1}
  main{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;padding:28px 22px;gap:22px}
  figure{max-width:min(100%,1200px)}
  img{display:block;max-width:100%;max-height:72vh;border-radius:10px;box-shadow:0 8px 40px #000c}
  .actions{display:flex;gap:12px}
  .btn{padding:9px 18px;border-radius:8px;text-decoration:none;font-weight:600;font-size:13px}
  .btn.primary{background:#3b82f6;color:#fff}
  .btn.ghost{border:1px solid #2c313b;color:#aeb6c2}
  .ad{width:min(100%,728px);min-height:90px;display:flex;align-items:center;justify-content:center;border:1px dashed #2c313b;border-radius:10px;color:#5c6472;font-size:12px}
  footer{padding:14px 22px;border-top:1px solid #22262e;color:#8b93a1;font-size:12px;display:flex;justify-content:space-between}
  footer a{color:#8b93a1}
</style></head>
<body>
<header><b>Quickshort</b><span class="tag">shared capture</span></header>
<main>
  <figure><img src="${raw}" alt="Shared screenshot"></figure>
  <div class="actions">
    <a class="btn primary" href="${raw}" download>Download</a>
    <a class="btn ghost" href="https://2tsoftwareapp.github.io/quickshort/">Get Quickshort</a>
  </div>
  <div class="ad">${ADS_ENABLED ? AD_SNIPPET : "Made with Quickshort — pixel-perfect capture, markup and share"}</div>
</main>
<footer><span>Expires 30 days after last view</span><a href="https://2tsoftwareapp.github.io/quickshort/privacy.html">Privacy</a></footer>
</body></html>`;
  // img-src self + object-src none: uploaded bytes render but can
  // never execute in this page's context. AdSense domains join the
  // allowlist only when the slot is actually enabled.
  const csp =
    "default-src 'none'; img-src 'self'; style-src 'unsafe-inline';" +
    (ADS_ENABLED
      ? " script-src https://pagead2.googlesyndication.com https://*.googlesyndication.com; frame-src https://googleads.g.doubleclick.net https://*.googlesyndication.com;"
      : " script-src 'none';");
  return new Response(html, {
    headers: {
      "content-type": "text/html; charset=utf-8",
      "content-security-policy": csp,
      "cache-control": "no-cache",
      "x-content-type-options": "nosniff",
    },
  });
}

async function revoke(req, { id, ext }, env, ctx) {
  const key = req.headers.get("x-delete-key") || "";
  if (!/^[a-f0-9]{32}$/.test(key)) return err(403, "bad delete key");
  const meta = await env.SHARE.get(metaKey(id)).then((v) => v && JSON.parse(v));
  if (!meta) return new Response("gone", { status: 404 });
  if ((await sha256hex(key)) !== meta.dk) return err(403, "bad delete key");
  await Promise.all([env.BLOB.delete(blobKey(id, ext)), env.SHARE.delete(metaKey(id))]);
  return Response.json({ ok: true });
}

function err(status, msg) {
  return Response.json({ error: msg }, { status });
}
