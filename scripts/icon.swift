// Makes the app icon's PNGs from a square picture of it (a dark rounded square on a light
// background, assets/icon/source.jpeg): the square cut out, fitted to Apple's macOS icon grid
// (a body of 824 of 1024 points, rounded corners, a soft shadow in the clear margin) and drawn
// at every size an .iconset holds. scripts/icon.sh runs it and iconutil.
// Usage: swift scripts/icon.swift <source image> <out.iconset>
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let arguments = CommandLine.arguments
guard arguments.count == 3 else {
    FileHandle.standardError.write("usage: swift scripts/icon.swift <source image> <out.iconset>\n".data(using: .utf8)!)
    exit(2)
}
let sourceURL = URL(fileURLWithPath: arguments[1])
let outDir = URL(fileURLWithPath: arguments[2])
let space = CGColorSpace(name: CGColorSpace.sRGB)!
let rgba = CGImageAlphaInfo.premultipliedLast.rawValue

guard let source = CGImageSourceCreateWithURL(sourceURL as CFURL, nil), let picture = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    FileHandle.standardError.write("icon: cannot read \(sourceURL.path)\n".data(using: .utf8)!)
    exit(1)
}

// The dark square's edges: the first and last dark pixels across the middle row and down the
// middle column (the background and its shadow are light).
let (w, h) = (picture.width, picture.height)
var pixels = [UInt8](repeating: 0, count: w * h * 4)
pixels.withUnsafeMutableBytes { buffer in
    let context = CGContext(data: buffer.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: space, bitmapInfo: rgba)!
    context.draw(picture, in: CGRect(x: 0, y: 0, width: w, height: h))
}
func dark(_ x: Int, _ y: Int) -> Bool {
    let i = (y * w + x) * 4
    return 0.299 * Double(pixels[i]) + 0.587 * Double(pixels[i + 1]) + 0.114 * Double(pixels[i + 2]) < 90
}
guard let left = (0..<w).first(where: { dark($0, h / 2) }), let right = (0..<w).last(where: { dark($0, h / 2) }),
      let top = (0..<h).first(where: { dark(w / 2, $0) }), let bottom = (0..<h).last(where: { dark(w / 2, $0) }) else {
    FileHandle.standardError.write("icon: no dark square in the picture\n".data(using: .utf8)!)
    exit(1)
}
// A few pixels in, past the edge's light fringe; the corners are cut by the mask below.
let inset = 4
let square = picture.cropping(to: CGRect(x: left + inset, y: top + inset, width: right - left - 2 * inset, height: bottom - top - 2 * inset))!
print("icon: the square is \(left)…\(right) × \(top)…\(bottom) of \(w) × \(h)")

// The 1024 master: Apple's grid puts the body at 100…924 with corners of about 185.
let body = CGRect(x: 100, y: 100, width: 824, height: 824)
let corners = CGPath(roundedRect: body, cornerWidth: 185, cornerHeight: 185, transform: nil)
let master = CGContext(data: nil, width: 1024, height: 1024, bitsPerComponent: 8, bytesPerRow: 0, space: space, bitmapInfo: rgba)!
master.interpolationQuality = .high
master.saveGState()
master.setShadow(offset: CGSize(width: 0, height: -10), blur: 24, color: CGColor(gray: 0, alpha: 0.35))
master.addPath(corners)
master.setFillColor(CGColor(gray: 0.05, alpha: 1))
master.fillPath()
master.restoreGState()
master.addPath(corners)
master.clip()
master.draw(square, in: body)
let icon = master.makeImage()!

func write(_ image: CGImage, size: Int, to name: String) {
    let context = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: space, bitmapInfo: rgba)!
    context.interpolationQuality = .high
    context.draw(image, in: CGRect(x: 0, y: 0, width: size, height: size))
    let url = outDir.appendingPathComponent(name)
    let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, context.makeImage()!, nil)
    guard CGImageDestinationFinalize(destination) else {
        FileHandle.standardError.write("icon: cannot write \(url.path)\n".data(using: .utf8)!)
        exit(1)
    }
}
try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    write(icon, size: points, to: "icon_\(points)x\(points).png")
    write(icon, size: points * 2, to: "icon_\(points)x\(points)@2x.png")
}
