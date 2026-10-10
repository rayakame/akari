/// Waits for `task`; cancelling the waiting test (its time limit) cancels `task` too, so a
/// task that never ends fails the test instead of hanging the run.
func finished(_ task: Task<Void, Never>?) async {
    guard let task else {
        return
    }
    await withTaskCancellationHandler {
        await task.value
    } onCancel: {
        task.cancel()
    }
}
