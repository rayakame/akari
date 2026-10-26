#!/usr/bin/env swift
// Regenerates the macOS app's icon files from assets/app-icon.svg; see apps/macos/README.md.
// Usage: swift apps/macos/make-icons.swift   (from anywhere; needs only Xcode's Swift)

import Foundation

let macos = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repo = macos.deletingLastPathComponent().deletingLastPathComponent()
let master = repo.appendingPathComponent("assets/app-icon.svg")
let app = macos.appendingPathComponent("Akari")

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("make-icons: \(message)\n".utf8))
    exit(1)
}

// The master is a 1024 square: a full-size rect in the brand color under the white logo.
guard let document = try? XMLDocument(contentsOf: master), let svg = document.rootElement() else {
    fail("can't read \(master.path)")
}
guard svg.attribute(forName: "viewBox")?.stringValue == "0 0 1024 1024" else {
    fail("app-icon.svg isn't a 0 0 1024 1024 square")
}
let shapes = svg.children?.compactMap { $0 as? XMLElement } ?? []
guard shapes.count == 2, let background = shapes.first, background.name == "rect",
    background.attribute(forName: "width")?.stringValue == "1024",
    background.attribute(forName: "height")?.stringValue == "1024",
    shapes[1].name == "path"
else {
    fail("app-icon.svg should hold a 1024 × 1024 rect and the logo's path, in that order")
}
guard let fill = background.attribute(forName: "fill")?.stringValue, fill.count == 7,
    fill.hasPrefix("#"), let rgb = UInt32(fill.dropFirst(), radix: 16)
else {
    fail("the rect's fill isn't a #rrggbb color")
}
let channels = [rgb >> 16, rgb >> 8 & 0xFF, rgb & 0xFF].map { String(format: "%.5f", Double($0) / 255) }

// The logo where the master places it, alone: the icon's layer and the app's logo image. Cut
// from the master's text, so the output doesn't depend on how Foundation writes XML.
guard let text = try? String(contentsOf: master, encoding: .utf8),
    let rect = text.range(of: #"<rect\b[^>]*/>"#, options: .regularExpression)
else {
    fail("can't find the rect in app-icon.svg's text")
}
var logo = text.replacingCharacters(in: rect, with: "")
if !logo.hasSuffix("\n") {
    logo += "\n"
}

let iconJSON = """
    {
      "fill" : {
        "solid" : "srgb:\(channels.joined(separator: ",")),1.00000"
      },
      "groups" : [
        {
          "layers" : [
            {
              "image-name" : "logo.svg",
              "name" : "logo"
            }
          ],
          "shadow" : {
            "kind" : "neutral",
            "opacity" : 0.5
          },
          "translucency" : {
            "enabled" : true,
            "value" : 0.5
          }
        }
      ],
      "supported-platforms" : {
        "squares" : [
          "macOS"
        ]
      }
    }

    """

let catalogJSON = """
    {
      "info" : {
        "author" : "xcode",
        "version" : 1
      }
    }

    """

let logoSetJSON = """
    {
      "images" : [
        {
          "filename" : "logo.svg",
          "idiom" : "universal"
        }
      ],
      "info" : {
        "author" : "xcode",
        "version" : 1
      },
      "properties" : {
        "preserves-vector-representation" : true,
        "template-rendering-intent" : "template"
      }
    }

    """

let outputs: [(String, String)] = [
    ("AppIcon.icon/icon.json", iconJSON),
    ("AppIcon.icon/Assets/logo.svg", logo),
    ("Assets.xcassets/Contents.json", catalogJSON),
    ("Assets.xcassets/AkariLogo.imageset/Contents.json", logoSetJSON),
    ("Assets.xcassets/AkariLogo.imageset/logo.svg", logo),
]
for (path, contents) in outputs {
    let url = app.appendingPathComponent(path)
    do {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(contents.utf8).write(to: url)
    } catch {
        fail("can't write \(url.path): \(error.localizedDescription)")
    }
    print(url.path.replacingOccurrences(of: repo.path + "/", with: ""))
}
