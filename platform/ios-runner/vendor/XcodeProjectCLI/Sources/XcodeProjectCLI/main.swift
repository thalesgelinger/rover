import Foundation

struct Args {
    let template: URL
    let output: URL
}

func parseArgs() -> Args {
    var template: URL?
    var output: URL?
    var iter = CommandLine.arguments.dropFirst().makeIterator()
    while let arg = iter.next() {
        switch arg {
        case "--template":
            if let val = iter.next() { template = URL(fileURLWithPath: val) }
        case "--out":
            if let val = iter.next() { output = URL(fileURLWithPath: val) }
        default:
            break
        }
    }
    guard let t = template, let o = output else {
        fputs("usage: xcodeprojectcli --template <dir> --out <dir>\n", stderr)
        exit(1)
    }
    return Args(template: t, output: o)
}

func copyTemplate(from: URL, to: URL) throws {
    let fm = FileManager.default
    if fm.fileExists(atPath: to.path) {
        try fm.removeItem(at: to)
    }
    try fm.createDirectory(at: to, withIntermediateDirectories: true)
    let enumerator = fm.enumerator(at: from, includingPropertiesForKeys: [.isDirectoryKey])!
    for case let url as URL in enumerator {
        let rel = url.path.replacingOccurrences(of: from.path, with: "").trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let dest = to.appendingPathComponent(rel)
        let attrs = try url.resourceValues(forKeys: [.isDirectoryKey])
        if attrs.isDirectory == true {
            try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        } else {
            try fm.copyItem(at: url, to: dest)
        }
    }
}

func writePBXProj(to output: URL) throws {
    let projDir = output.appendingPathComponent("RoverApp.xcodeproj")
    let fm = FileManager.default
    try fm.createDirectory(at: projDir, withIntermediateDirectories: true)
    let pbx = projDir.appendingPathComponent("project.pbxproj")
    try pbxprojContent.data(using: .utf8)!.write(to: pbx)
}

let args = parseArgs()
do {
    try copyTemplate(from: args.template, to: args.output)
    try writePBXProj(to: args.output)
    print("generated project at \(args.output.path)")
} catch {
    fputs("error: \(error)\n", stderr)
    exit(1)
}

let pbxprojContent = """
// !$*UTF8*$!
{
\tarchiveVersion = 1;
\tclasses = {
\t};
\tobjectVersion = 56;
\tobjects = {

/* Begin PBXBuildFile section */
\t\t1A2B3C4D5E6F001100000001 /* RoverAppApp.swift in Sources */ = {isa = PBXBuildFile; fileRef = 1A2B3C4D5E6F001000000001 /* RoverAppApp.swift */; };
\t\t1A2B3C4D5E6F001300000001 /* ContentView.swift in Sources */ = {isa = PBXBuildFile; fileRef = 1A2B3C4D5E6F001200000001 /* ContentView.swift */; };
\t\t1A2B3C4D5E6F001500000001 /* Assets.xcassets in Resources */ = {isa = PBXBuildFile; fileRef = 1A2B3C4D5E6F001400000001 /* Assets.xcassets */; };
\t\t1A2B3C4D5E6F001800000001 /* Preview Assets.xcassets in Resources */ = {isa = PBXBuildFile; fileRef = 1A2B3C4D5E6F001700000001 /* Preview Assets.xcassets */; };
/* End PBXBuildFile section */

/* Begin PBXFileReference section */
\t\t1A2B3C4D5E6F000F00000001 /* RoverApp.app */ = {isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = RoverApp.app; sourceTree = BUILT_PRODUCTS_DIR; };
\t\t1A2B3C4D5E6F001000000001 /* RoverAppApp.swift */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = RoverAppApp.swift; sourceTree = "<group>"; };
\t\t1A2B3C4D5E6F001200000001 /* ContentView.swift */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = ContentView.swift; sourceTree = "<group>"; };
\t\t1A2B3C4D5E6F001400000001 /* Assets.xcassets */ = {isa = PBXFileReference; lastKnownFileType = folder.assetcatalog; path = Assets.xcassets; sourceTree = "<group>"; };
\t\t1A2B3C4D5E6F001600000001 /* Info.plist */ = {isa = PBXFileReference; lastKnownFileType = text.plist.xml; path = Info.plist; sourceTree = "<group>"; };
\t\t1A2B3C4D5E6F001700000001 /* Preview Assets.xcassets */ = {isa = PBXFileReference; lastKnownFileType = folder.assetcatalog; path = "Preview Assets.xcassets"; sourceTree = "<group>"; };
/* End PBXFileReference section */

/* Begin PBXFrameworksBuildPhase section */
\t\t1A2B3C4D5E6F000B00000001 /* Frameworks */ = {
\t\t\tisa = PBXFrameworksBuildPhase;
\t\t\tbuildActionMask = 2147483647;
\t\t\tfiles = (
\t\t\t);
\t\t\trunOnlyForDeploymentPostprocessing = 0;
\t\t};
/* End PBXFrameworksBuildPhase section */

/* Begin PBXGroup section */
\t\t1A2B3C4D5E6F000500000001 = {
\t\t\tisa = PBXGroup;
\t\t\tchildren = (
\t\t\t\t1A2B3C4D5E6F000E00000001 /* RoverApp */,
\t\t\t\t1A2B3C4D5E6F000F00000001 /* RoverApp.app */,
\t\t\t);
\t\t\tsourceTree = "<group>";
\t\t};
\t\t1A2B3C4D5E6F000E00000001 /* RoverApp */ = {
\t\t\tisa = PBXGroup;
\t\t\tchildren = (
\t\t\t\t1A2B3C4D5E6F001000000001 /* RoverAppApp.swift */,
\t\t\t\t1A2B3C4D5E6F001200000001 /* ContentView.swift */,
\t\t\t\t1A2B3C4D5E6F001400000001 /* Assets.xcassets */,
\t\t\t\t1A2B3C4D5E6F001700000001 /* Preview Assets.xcassets */,
\t\t\t\t1A2B3C4D5E6F001600000001 /* Info.plist */,
\t\t\t);
\t\t\tpath = RoverApp;
\t\t\tsourceTree = "<group>";
\t\t};
/* End PBXGroup section */

/* Begin PBXNativeTarget section */
\t\t1A2B3C4D5E6F000A00000001 /* RoverApp */ = {
\t\t\tisa = PBXNativeTarget;
\t\t\tbuildConfigurationList = 1A2B3C4D5E6F001D00000001 /* Build configuration list for PBXNativeTarget \"RoverApp\" */;
\t\t\tbuildPhases = (
\t\t\t\t1A2B3C4D5E6F000700000001 /* Sources */,
\t\t\t\t1A2B3C4D5E6F000B00000001 /* Frameworks */,
\t\t\t\t1A2B3C4D5E6F000C00000001 /* Resources */,
\t\t\t);
\t\t\tbuildRules = (
\t\t\t);
\t\t\tdependencies = (
\t\t\t);
\t\t\tname = RoverApp;
\t\t\tproductName = RoverApp;
\t\t\tproductReference = 1A2B3C4D5E6F000F00000001 /* RoverApp.app */;
\t\t\tproductType = "com.apple.product-type.application";
\t\t};
/* End PBXNativeTarget section */

/* Begin PBXProject section */
\t\t1A2B3C4D5E6F000600000001 /* Project object */ = {
\t\t\tisa = PBXProject;
\t\t\tattributes = {
\t\t\t\tBuildIndependentTargetsInParallel = 1;
\t\t\t\tLastSwiftUpdateCheck = 1430;
\t\t\t\tLastUpgradeCheck = 1430;
\t\t\t\tTargetAttributes = {
\t\t\t\t\t1A2B3C4D5E6F000A00000001 = {
\t\t\t\t\t\tCreatedOnToolsVersion = 14.3;
\t\t\t\t\t};
\t\t\t\t};
\t\t\t};
\t\t\tbuildConfigurationList = 1A2B3C4D5E6F000800000001 /* Build configuration list for PBXProject \"RoverApp\" */;
\t\t\tcompatibilityVersion = "Xcode 14.0";
\t\t\tdevelopmentRegion = en;
\t\t\thasScannedForEncodings = 0;
\t\t\tknownRegions = (
\t\t\t\ten,
\t\t\t);
\t\t\tmainGroup = 1A2B3C4D5E6F000500000001;
\t\t\tproductRefGroup = 1A2B3C4D5E6F000500000001;
\t\t\tprojectDirPath = "";
\t\t\tprojectRoot = "";
\t\t\ttargets = (
\t\t\t\t1A2B3C4D5E6F000A00000001 /* RoverApp */,
\t\t\t);
\t\t};
/* End PBXProject section */

/* Begin PBXResourcesBuildPhase section */
\t\t1A2B3C4D5E6F000C00000001 /* Resources */ = {
\t\t\tisa = PBXResourcesBuildPhase;
\t\t\tbuildActionMask = 2147483647;
\t\t\tfiles = (
\t\t\t\t1A2B3C4D5E6F001500000001 /* Assets.xcassets in Resources */,
\t\t\t\t1A2B3C4D5E6F001800000001 /* Preview Assets.xcassets in Resources */,
\t\t\t);
\t\t\trunOnlyForDeploymentPostprocessing = 0;
\t\t};
/* End PBXResourcesBuildPhase section */

/* Begin PBXSourcesBuildPhase section */
\t\t1A2B3C4D5E6F000700000001 /* Sources */ = {
\t\t\tisa = PBXSourcesBuildPhase;
\t\t\tbuildActionMask = 2147483647;
\t\t\tfiles = (
\t\t\t\t1A2B3C4D5E6F001300000001 /* ContentView.swift in Sources */,
\t\t\t\t1A2B3C4D5E6F001100000001 /* RoverAppApp.swift in Sources */,
\t\t\t);
\t\t\trunOnlyForDeploymentPostprocessing = 0;
\t\t};
/* End PBXSourcesBuildPhase section */

/* Begin XCBuildConfiguration section */
\t\t1A2B3C4D5E6F001900000001 /* Debug */ = {
\t\t\tisa = XCBuildConfiguration;
\t\t\tbuildSettings = {
\t\t\t\tALWAYS_SEARCH_USER_PATHS = NO;
\t\t\t\tCODE_SIGN_STYLE = Automatic;
\t\t\t\tCURRENT_PROJECT_VERSION = 1;
\t\t\t\tINFOPLIST_FILE = RoverApp/Info.plist;
\t\t\t\tIPHONEOS_DEPLOYMENT_TARGET = 16.0;
\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = dev.rover.app;
\t\t\t\tPRODUCT_NAME = "$(TARGET_NAME)";
\t\t\t\tSDKROOT = iphoneos;
\t\t\t\tSUPPORTED_PLATFORMS = "iphonesimulator iphoneos";
\t\t\t\tTARGETED_DEVICE_FAMILY = "1";
\t\t\t};
\t\t\tname = Debug;
\t\t};
\t\t1A2B3C4D5E6F001A00000001 /* Release */ = {
\t\t\tisa = XCBuildConfiguration;
\t\t\tbuildSettings = {
\t\t\t\tALWAYS_SEARCH_USER_PATHS = NO;
\t\t\t\tCODE_SIGN_STYLE = Automatic;
\t\t\t\tCURRENT_PROJECT_VERSION = 1;
\t\t\t\tINFOPLIST_FILE = RoverApp/Info.plist;
\t\t\t\tIPHONEOS_DEPLOYMENT_TARGET = 16.0;
\t\t\t\tPRODUCT_BUNDLE_IDENTIFIER = dev.rover.app;
\t\t\t\tPRODUCT_NAME = "$(TARGET_NAME)";
\t\t\t\tSDKROOT = iphoneos;
\t\t\t\tSUPPORTED_PLATFORMS = "iphonesimulator iphoneos";
\t\t\t\tTARGETED_DEVICE_FAMILY = "1";
\t\t\t};
\t\t\tname = Release;
\t\t};
/* End XCBuildConfiguration section */

/* Begin XCConfigurationList section */
\t\t1A2B3C4D5E6F000800000001 /* Build configuration list for PBXProject \"RoverApp\" */ = {
\t\t\tisa = XCConfigurationList;
\t\t\tbuildConfigurations = (
\t\t\t\t1A2B3C4D5E6F001900000001 /* Debug */,
\t\t\t\t1A2B3C4D5E6F001A00000001 /* Release */,
\t\t\t);
\t\t\tdefaultConfigurationIsVisible = 0;
\t\t\tdefaultConfigurationName = Debug;
\t\t};
\t\t1A2B3C4D5E6F001D00000001 /* Build configuration list for PBXNativeTarget \"RoverApp\" */ = {
\t\t\tisa = XCConfigurationList;
\t\t\tbuildConfigurations = (
\t\t\t\t1A2B3C4D5E6F001900000001 /* Debug */,
\t\t\t\t1A2B3C4D5E6F001A00000001 /* Release */,
\t\t\t);
\t\t\tdefaultConfigurationIsVisible = 0;
\t\t\tdefaultConfigurationName = Debug;
\t\t};
/* End XCConfigurationList section */
\t};
\trootObject = 1A2B3C4D5E6F000600000001 /* Project object */;
}
"""
