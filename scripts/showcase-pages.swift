// Render every page of a PDF to PNG, for looking at the showcase.
//
//     swift scripts/showcase-pages.swift target/showcase.pdf target/showcase
//
// macOS only (PDFKit). Writes page1.png, page2.png, … into the directory.
import AppKit
import PDFKit

let args = CommandLine.arguments
guard args.count == 3, let doc = PDFDocument(url: URL(fileURLWithPath: args[1])) else {
    FileHandle.standardError.write("usage: showcase-pages <file.pdf> <out-dir>\n".data(using: .utf8)!)
    exit(1)
}
try FileManager.default.createDirectory(atPath: args[2], withIntermediateDirectories: true)
for i in 0..<doc.pageCount {
    let image = doc.page(at: i)!.thumbnail(of: NSSize(width: 1400, height: 1980), for: .mediaBox)
    let png = NSBitmapImageRep(data: image.tiffRepresentation!)!.representation(using: .png, properties: [:])!
    try png.write(to: URL(fileURLWithPath: "\(args[2])/page\(i + 1).png"))
}
print("wrote \(doc.pageCount) pages to \(args[2])")
