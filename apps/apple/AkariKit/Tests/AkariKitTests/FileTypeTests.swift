import Testing

@testable import AkariKit

// The official client's `filesize` (module 796873): base 1024, "bytes" without a singular, two
// decimals from KB on, a thousands comma.
struct FileSizeTests {
    @Test(arguments: [
        (0, "0 bytes"), (1, "1 bytes"), (512, "512 bytes"), (1023, "1,023 bytes"),
        (1024, "1.00 KB"), (1536, "1.50 KB"), (1_572_864, "1.50 MB"), (1_048_575, "1,024.00 KB"),
        (1_073_741_824, "1.00 GB"), (3_298_534_883_328, "3.00 TB"),
    ] as [(UInt64, String)])
    func sizesReadAsDiscordWritesThem(bytes: UInt64, text: String) {
        #expect(FileSize.text(bytes) == text)
    }
}

// Akari's own symbols for Discord's file-type icons (its SVGs are Discord's assets).
struct FileTypeTests {
    @Test(arguments: [
        ("notes.zip", "doc.zipper"), ("a.tar.GZ", "doc.zipper"), ("song.mp3", "waveform"),
        ("clip.mp4", "film"), ("photo.png", "photo"), ("main.rs", "chevron.left.forwardslash.chevron.right"),
        ("page.html", "chevron.left.forwardslash.chevron.right"), ("readme.txt", "doc.text"),
        ("paper.pdf", "doc.richtext"), ("sheet.xlsx", "tablecells"), ("data.csv", "tablecells"),
        ("letter.docx", "doc.richtext"), ("noextension", "doc"), ("zip", "doc"), ("design.sketch", "doc"),
    ])
    func filesGetASymbolByExtension(name: String, symbol: String) {
        #expect(FileType.symbol(forFileNamed: name) == symbol)
    }
}
