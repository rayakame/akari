import AppKit

// Dark by default, as CLAUDE.md asks.
enum ThemeChoice: String, CaseIterable, Identifiable {
    case dark
    case light
    case system

    static let key = "theme"

    var id: Self { self }

    var title: String {
        switch self {
        case .dark: "Dark"
        case .light: "Light"
        case .system: "Match System"
        }
    }

    var appearance: NSAppearance? {
        switch self {
        case .dark: NSAppearance(named: .darkAqua)
        case .light: NSAppearance(named: .aqua)
        case .system: nil
        }
    }

    static func stored(in defaults: UserDefaults) -> ThemeChoice {
        defaults.string(forKey: key).flatMap(ThemeChoice.init(rawValue:)) ?? .dark
    }

    func store(in defaults: UserDefaults) {
        defaults.set(rawValue, forKey: Self.key)
    }
}
