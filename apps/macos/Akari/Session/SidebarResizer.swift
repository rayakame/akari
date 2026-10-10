import AppKit
import SwiftUI

struct SidebarResizer: View {
    @Binding var width: CGFloat
    @State private var start: CGFloat?

    var body: some View {
        Color.clear
            .frame(width: 4)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside {
                    NSCursor.resizeLeftRight.push()
                } else {
                    NSCursor.pop()
                }
            }
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .global)
                    .onChanged { drag in
                        let base = start ?? width
                        start = base
                        width = SidebarWidth.clamped(base + drag.translation.width)
                    }
                    .onEnded { _ in
                        start = nil
                        SidebarWidth.store(width, in: .standard)
                    }
            )
    }
}
