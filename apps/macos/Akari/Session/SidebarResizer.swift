import AppKit
import SwiftUI

// The sidebar's edge: its resize cursor is held through a drag and goes when the edge does, as
// the split's gap's (ResizeCursor).
struct SidebarResizer: View {
    @Binding var width: CGFloat
    @State private var start: CGFloat?
    @State private var cursor: ResizeCursor
    private let store: (CGFloat) -> Void

    init(
        width: Binding<CGFloat>, cursor: ResizeCursor = ResizeCursor(),
        store: @escaping (CGFloat) -> Void = { SidebarWidth.store($0, in: .standard) }
    ) {
        _width = width
        _cursor = State(initialValue: cursor)
        self.store = store
    }

    var body: some View {
        Color.clear
            .frame(width: 4)
            .contentShape(Rectangle())
            .onHover { cursor.hover($0) }
            .onDisappear { cursor.release() }
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .global)
                    .onChanged { drag in
                        if start == nil {
                            cursor.drag(true)
                        }
                        let base = start ?? width
                        start = base
                        width = SidebarWidth.clamped(base + drag.translation.width)
                    }
                    .onEnded { _ in
                        start = nil
                        cursor.drag(false)
                        store(width)
                    }
            )
    }
}
