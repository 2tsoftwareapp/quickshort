function Component() {}

function desktopValue(value) {
    return value.replace(/\\/g, "\\\\").replace(/\r/g, "\\r").replace(/\n/g, "\\n");
}

Component.prototype.createOperations = function() {
    component.createOperations();
    var target = installer.value("TargetDir");
    var appImage = target + "/Quickshort.AppImage";
    var exec = '"' + appImage.replace(/[\\`"$]/g, "\\$&").replace(/%/g, "%%") + '"';
    component.addOperation("Execute", "/bin/chmod", "0755", appImage);
    component.addOperation("CreateDesktopEntry", "ai.quickshort.AppImage.desktop", [
        "Type=Application",
        "Name=Quickshort",
        "Comment=Capture and annotate screenshots",
        "Exec=/usr/bin/env APPIMAGE_EXTRACT_AND_RUN=1 " + desktopValue(exec),
        "Icon=" + desktopValue(target + "/quickshort.png"),
        "Categories=Utility;Graphics;",
        "StartupNotify=false",
        "StartupWMClass=quickshort"
    ].join("\n"));
};
