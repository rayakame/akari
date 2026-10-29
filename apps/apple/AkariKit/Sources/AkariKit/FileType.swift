import Foundation

/// A file's size as the official client words it (its `filesize`, module 796873).
public enum FileSize {
    private static let units = ["bytes", "KB", "MB", "GB", "TB", "PB"]

    public static func text(_ bytes: UInt64) -> String {
        guard bytes > 0 else {
            return "0 bytes"
        }
        var value = Double(bytes)
        var unit = 0
        while value >= 1024, unit < units.count - 1 {
            value /= 1024
            unit += 1
        }
        let formatter = NumberFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.numberStyle = .decimal
        formatter.usesGroupingSeparator = true
        formatter.groupingSeparator = ","
        formatter.decimalSeparator = "."
        let decimals = unit == 0 ? 0 : 2
        formatter.minimumFractionDigits = decimals
        formatter.maximumFractionDigits = decimals
        formatter.roundingMode = .halfUp
        let number = formatter.string(from: NSNumber(value: value)) ?? String(value)
        return "\(number) \(units[unit])"
    }
}

/// The symbol a file row shows, by the file's extension.
public enum FileType {
    /// The name Discord shows for a file: the title with the file's extension when it gave one.
    public static func shownName(_ attachment: Attachment) -> String {
        guard let title = attachment.title, !title.isEmpty else {
            return attachment.filename
        }
        let parts = attachment.filename.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count > 1, let last = parts.last else {
            return title
        }
        return "\(title).\(last)"
    }

    public static func symbol(forFileNamed name: String) -> String {
        let parts = name.lowercased().split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count > 1, let last = parts.last else {
            return "doc"
        }
        switch String(last) {
        case "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "zst":
            return "doc.zipper"
        case "mp3", "m4a", "ogg", "opus", "wav", "flac", "aac", "aiff":
            return "waveform"
        case "mp4", "webm", "mov", "qt", "mkv", "avi", "m4v":
            return "film"
        case "png", "jpg", "jpeg", "jfif", "webp", "gif", "heic", "heif", "dng", "avif", "svg", "bmp", "tiff":
            return "photo"
        case "txt", "md", "log", "rtf":
            return "doc.text"
        case "pdf", "doc", "docx", "odt", "pages":
            return "doc.richtext"
        case "xls", "xlsx", "csv", "ods", "numbers", "tsv":
            return "tablecells"
        case "rs", "swift", "c", "h", "cpp", "hpp", "cs", "java", "kt", "js", "ts", "jsx", "tsx", "py", "rb",
            "go", "php", "sh", "zsh", "bash", "lua", "json", "yaml", "yml", "toml", "xml", "html", "css",
            "scss", "sql", "ini", "cfg", "diff", "patch":
            return "chevron.left.forwardslash.chevron.right"
        default:
            return "doc"
        }
    }
}
