import AkariFFI
import Foundation

// UniFFI's Swift glue never cancels a Rust call with its task, and a running `result()` holds
// the pending object, so a cancelled task has to say so itself.
extension Account {
    /// The image for `request`; cancelling the task cancels the request, and the download
    /// stops when nobody else waits for it.
    public func image(_ request: ImageRequest) async throws -> ImageFile {
        try await requestImage(request: request).awaitingCancellation()
    }

    /// Saves the original to `destination`, replacing it only once the original is whole;
    /// cancelling the task leaves the file as it was. Returns the bytes written.
    public func saveOriginal(_ key: ImageKey, to destination: URL) async throws -> UInt64 {
        try await saveOriginal(key: key, destination: destination.path).awaitingCancellation()
    }
}

extension PendingImage {
    /// `result()`, with the task's cancellation reaching the request.
    public func awaitingCancellation() async throws -> ImageFile {
        try await withTaskCancellationHandler {
            try await result()
        } onCancel: {
            cancel()
        }
    }
}

extension PendingSave {
    /// `result()`, with the task's cancellation reaching the save.
    public func awaitingCancellation() async throws -> UInt64 {
        try await withTaskCancellationHandler {
            try await result()
        } onCancel: {
            cancel()
        }
    }
}
