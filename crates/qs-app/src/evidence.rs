//! Evidence session — formal test evidence without leaving the one capture
//! flow. While a session runs, every saved capture lands in the session
//! folder as `TC-001_01.png`, `TC-001_02.png`, …; "Next test case" and "End"
//! stitch the current case into one tall `TC-001_all.png` for Excel/Jira.

use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};

/// White gap between stitched shots, px.
const GAP: u32 = 24;

pub struct Session {
    pub dir: PathBuf,
    case_no: u32,
    shots: u32,
}

impl Session {
    pub fn start(save_dir: &Path) -> Result<Self> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let dir = save_dir.join(format!("evidence-{ts}"));
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        Ok(Self {
            dir,
            case_no: 1,
            shots: 0,
        })
    }

    pub fn case_id(&self) -> String {
        format!("TC-{:03}", self.case_no)
    }

    /// Tray label, e.g. "TC-001 · 2 shots".
    pub fn label(&self) -> String {
        let s = if self.shots == 1 { "" } else { "s" };
        format!("{} · {} shot{s}", self.case_id(), self.shots)
    }

    /// File for the next saved capture of the current case.
    pub fn next_path(&mut self) -> PathBuf {
        self.shots += 1;
        self.shot_path(self.shots)
    }

    fn shot_path(&self, n: u32) -> PathBuf {
        self.dir.join(format!("{}_{:02}.png", self.case_id(), n))
    }

    /// Close the current case and start the next one.
    pub fn next_case(&mut self) -> Option<PathBuf> {
        let out = self.stitch();
        self.case_no += 1;
        self.shots = 0;
        out
    }

    /// End the session; returns the session folder.
    pub fn finish(self) -> PathBuf {
        self.stitch();
        self.dir
    }

    /// Stack the case's shots top to bottom, left-aligned on white.
    /// Nothing to do for fewer than two shots.
    fn stitch(&self) -> Option<PathBuf> {
        let imgs: Vec<RgbaImage> = (1..=self.shots)
            .filter_map(|n| image::open(self.shot_path(n)).ok())
            .map(|i| i.to_rgba8())
            .collect();
        if imgs.len() < 2 {
            return None;
        }
        let w = imgs.iter().map(|i| i.width()).max()?;
        let h = imgs.iter().map(|i| i.height()).sum::<u32>() + GAP * (imgs.len() as u32 - 1);
        let mut out = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        let mut y = 0i64;
        for i in &imgs {
            image::imageops::overlay(&mut out, i, 0, y);
            y += (i.height() + GAP) as i64;
        }
        let p = self.dir.join(format!("{}_all.png", self.case_id()));
        match out.save(&p) {
            Ok(()) => {
                eprintln!("evidence: stitched {}", p.display());
                Some(p)
            }
            Err(e) => {
                eprintln!("evidence: stitch {}: {e}", p.display());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_shots_per_case_and_stitches() {
        let root = std::env::temp_dir().join(format!("qs-evidence-{}", std::process::id()));
        let mut s = Session::start(&root).unwrap();
        let a = s.next_path();
        let b = s.next_path();
        assert!(a.ends_with("TC-001_01.png") && b.ends_with("TC-001_02.png"));
        RgbaImage::from_pixel(40, 10, Rgba([255, 0, 0, 255])).save(&a).unwrap();
        RgbaImage::from_pixel(30, 20, Rgba([0, 0, 255, 255])).save(&b).unwrap();
        assert_eq!(s.label(), "TC-001 · 2 shots");
        let all = s.next_case().unwrap();
        let img = image::open(&all).unwrap().to_rgba8();
        assert_eq!((img.width(), img.height()), (40, 10 + GAP + 20));
        assert_eq!(img.get_pixel(35, 12 + GAP).0, [255, 255, 255, 255]);
        assert!(s.next_path().ends_with("TC-002_01.png"));
        assert!(s.next_case().is_none(), "one shot needs no stitch");
        let _ = std::fs::remove_dir_all(root);
    }
}
