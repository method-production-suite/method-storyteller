// Draws the app icon: the mark on a light rounded square, on the macOS icon grid (an 824
// point square centred in 1024). Run by make-icons.sh; usage: swift render-icon.swift
// <mark.svg> <out.png>. Needs macOS 13 or later (NSImage reads SVG).
import AppKit

let args = CommandLine.arguments
guard args.count == 3, let mark = NSImage(contentsOf: URL(fileURLWithPath: args[1])) else {
    FileHandle.standardError.write("usage: swift render-icon.swift <mark.svg> <out.png>\n".data(using: .utf8)!)
    exit(1)
}
let size = 1024
guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0)
else { exit(1) }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let tile = NSRect(x: 100, y: 100, width: 824, height: 824)
NSColor(red: 0.98, green: 0.965, blue: 0.955, alpha: 1).setFill()
NSBezierPath(roundedRect: tile, xRadius: 185, yRadius: 185).fill()
mark.draw(in: tile.insetBy(dx: 90, dy: 90))
NSGraphicsContext.restoreGraphicsState()
guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
do { try png.write(to: URL(fileURLWithPath: args[2])) } catch { exit(1) }
