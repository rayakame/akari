import AkariKit
import AppKit

/// What the viewer's buttons and More menu do with the item showing when they were pressed
/// (F-7): the original in the browser or on the pasteboard, saved where the save panel says.
@MainActor
final class ViewerActions {
    let model: ViewerModel
    var pasteboard: NSPasteboard
    var openURL: (URL) -> Void
    /// The save panel's answer for a suggested name; nil when cancelled.
    var chooseSaveURL: (String) async -> URL?
    var onFailure: (String) -> Void

    static let saveFailed = "The file couldn't be saved."
    static let copyFailed = "The image couldn't be copied."

    init(
        model: ViewerModel, pasteboard: NSPasteboard, openURL: @escaping (URL) -> Void,
        chooseSaveURL: @escaping (String) async -> URL?, onFailure: @escaping (String) -> Void
    ) {
        self.model = model
        self.pasteboard = pasteboard
        self.openURL = openURL
        self.chooseSaveURL = chooseSaveURL
        self.onFailure = onFailure
    }

    func openInBrowser() async {
        if let url = await model.originalURL(of: model.current), !Task.isCancelled {
            openURL(url)
        }
    }

    func copyLink() async {
        if let url = await model.originalURL(of: model.current), !Task.isCancelled {
            copy(url.absoluteString)
        }
    }

    func copy(_ value: String) {
        pasteboard.clearContents()
        pasteboard.setString(value, forType: .string)
    }

    func save() async {
        let item = model.current
        let name: String
        if let filename = item.filename {
            name = filename
        } else {
            name = await model.originalURL(of: item)?.lastPathComponent ?? "image"
        }
        guard let destination = await chooseSaveURL(name) else {
            return
        }
        do {
            _ = try await model.save(item, to: destination)
        } catch is CancellationError {
        } catch {
            onFailure(Self.saveFailed)
        }
    }

    // The original through a file of our own: NSImage reads every format the Mac decodes.
    func copyImage() async {
        let item = model.current
        let file = FileManager.default.temporaryDirectory.appending(
            path: "akari-copy-\(UUID().uuidString)", directoryHint: .notDirectory)
        defer {
            try? FileManager.default.removeItem(at: file)
        }
        do {
            _ = try await model.save(item, to: file)
        } catch is CancellationError {
            return
        } catch {
            onFailure(Self.copyFailed)
            return
        }
        guard !Task.isCancelled else {
            return
        }
        guard let image = NSImage(contentsOf: file) else {
            onFailure(Self.copyFailed)
            return
        }
        pasteboard.clearContents()
        pasteboard.writeObjects([image])
    }
}
