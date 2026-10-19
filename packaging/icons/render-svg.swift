// Renders SVG drawings to square PNGs through AppKit, for scripts/make-icons.py.
// Usage: swift render-svg.swift <in.svg> <out.png> <pixels> [<in.svg> <out.png> <pixels>]...
// Needs macOS 13 or later (NSImage reads SVG).
import AppKit

let args = Array(CommandLine.arguments.dropFirst())
guard !args.isEmpty, args.count % 3 == 0 else {
    FileHandle.standardError.write("usage: swift render-svg.swift (<in.svg> <out.png> <pixels>)...\n".data(using: .utf8)!)
    exit(1)
}
for job in stride(from: 0, to: args.count, by: 3) {
    guard let image = NSImage(contentsOf: URL(fileURLWithPath: args[job])), let size = Int(args[job + 2]),
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0)
    else {
        FileHandle.standardError.write("could not render \(args[job])\n".data(using: .utf8)!)
        exit(1)
    }
    rep.size = NSSize(width: size, height: size)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSGraphicsContext.current?.imageInterpolation = .high
    image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
    NSGraphicsContext.restoreGraphicsState()
    guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
    do { try png.write(to: URL(fileURLWithPath: args[job + 1])) } catch { exit(1) }
}
