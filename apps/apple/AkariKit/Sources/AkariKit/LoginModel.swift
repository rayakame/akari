import Foundation
import Observation

/// Why the login screen shows instead of the last session.
public enum LoginNotice: Equatable, Sendable {
    case sessionExpired
    case tokenUnreadable
}

/// The login screen: a QR code login running beside an email/password form. Whichever
/// finishes first logs in and cancels the other.
@MainActor @Observable
public final class LoginModel {
    public enum QrState: Equatable {
        case connecting
        case code(url: String)
        case scanned(ScannedUser)
        case captchaUnsupported
        case failed(LoginError)
    }

    public enum Step: Equatable {
        case credentials
        case mfa(MfaChallenge, method: MfaMethod)
        case newLocation(NewLocation)
        case captchaUnsupported
    }

    public var login = ""
    public var password = ""
    /// The MFA code, the SMS code, or the address Discord's email link opened.
    public var code = ""
    public private(set) var qr: QrState = .connecting
    public private(set) var step: Step = .credentials
    public private(set) var isSubmitting = false
    /// Discord's message for the fields, shown in their labels.
    public private(set) var fieldError: String?
    /// Shown above the button: rate limits, disabled accounts, network errors.
    public private(set) var message: String?
    public private(set) var rateLimitedUntil: Date?
    public let notice: LoginNotice?

    @ObservationIgnored var qrLoop: Task<Void, Never>?
    @ObservationIgnored private let client: DiscordClient
    @ObservationIgnored private let now: () -> Date
    @ObservationIgnored private let onLogin: @MainActor (LoginSuccess) async -> Void
    @ObservationIgnored private var passwordLogin: PasswordLogin?
    @ObservationIgnored private var qrLogin: QrLogin?
    @ObservationIgnored private var loggedIn = false

    /// `onLogin` runs once, for the flow that finishes first; the login waits for it.
    public convenience init(
        client: DiscordClient, notice: LoginNotice?,
        onLogin: @escaping @MainActor (LoginSuccess) async -> Void
    ) {
        self.init(client: client, notice: notice, now: Date.init, onLogin: onLogin)
    }

    init(
        client: DiscordClient, notice: LoginNotice?, now: @escaping () -> Date,
        onLogin: @escaping @MainActor (LoginSuccess) async -> Void
    ) {
        self.client = client
        self.notice = notice
        self.now = now
        self.onLogin = onLogin
    }

    public func canSubmit(at now: Date) -> Bool {
        if isSubmitting || loggedIn {
            return false
        }
        if let until = rateLimitedUntil, now < until {
            return false
        }
        switch step {
        case .credentials: return !login.isEmpty && !password.isEmpty
        case .mfa, .newLocation: return !code.isEmpty
        case .captchaUnsupported: return false
        }
    }

    /// Starts the QR login; the screen appeared.
    public func appear() {
        guard qrLogin == nil, !loggedIn else {
            return
        }
        startQr()
    }

    public func restartQr() {
        guard !loggedIn else {
            return
        }
        stopQr()
        startQr()
    }

    /// Submits the current step: credentials, MFA code, or new-location link or code.
    public func submit() async {
        guard canSubmit(at: now()) else {
            return
        }
        let flow = passwordLogin ?? client.passwordLogin()
        passwordLogin = flow
        await run {
            switch self.step {
            case .credentials:
                try await flow.submit(login: self.login, password: self.password)
            case .mfa(_, let method):
                try await flow.submitMfa(method: method, code: self.code)
            case .newLocation(.email):
                try await flow.confirmNewLocation(linkOrToken: self.code)
            case .newLocation(.phone):
                try await flow.verifyPhone(code: self.code)
            case .captchaUnsupported:
                nil
            }
        }
    }

    /// SMS texts a code first.
    public func choose(_ method: MfaMethod) async {
        guard case .mfa(let challenge, _) = step, !isSubmitting, let flow = passwordLogin else {
            return
        }
        code = ""
        fieldError = nil
        guard method == .sms else {
            step = .mfa(challenge, method: method)
            return
        }
        await run {
            let next = try await flow.sendMfaSms()
            if case .mfa(let challenge) = next {
                self.step = .mfa(challenge, method: .sms)
                return nil
            }
            return next
        }
    }

    public func backToForm() {
        passwordLogin?.cancel()
        passwordLogin = nil
        step = .credentials
        code = ""
        fieldError = nil
        message = nil
    }

    public func dismissCaptcha() {
        step = .credentials
    }

    /// Cancels both flows; the screen went away.
    public func disappear() {
        stopQr()
        passwordLogin?.cancel()
        passwordLogin = nil
    }

    // `request` returns nil when it already set the step.
    private func run(_ request: @MainActor () async throws -> LoginStep?) async {
        isSubmitting = true
        fieldError = nil
        message = nil
        rateLimitedUntil = nil
        defer {
            isSubmitting = false
        }
        do {
            guard let next = try await request() else {
                return
            }
            switch next {
            case .done(let success):
                await finish(success, cancelling: .qr)
            case .captcha:
                step = .captchaUnsupported
            case .mfa(let challenge):
                code = ""
                let preferred = [MfaMethod.totp, .sms, .backup].first(where: challenge.methods.contains)
                step = .mfa(challenge, method: preferred ?? challenge.methods.first ?? .totp)
            case .newLocation(let via):
                code = ""
                step = .newLocation(via)
            }
        } catch {
            show(error as? LoginError ?? .UnexpectedResponse)
        }
    }

    private func show(_ error: LoginError) {
        switch error {
        case .Cancelled:
            break
        case .InvalidCredentials(let text):
            fieldError = text
        case .InvalidMfaCode:
            fieldError = error.localizedDescription
        case .RateLimited(let retryAfter?, _):
            rateLimitedUntil = now().addingTimeInterval(retryAfter)
        case .Expired:
            step = .credentials
            code = ""
            message = error.localizedDescription
        default:
            message = error.localizedDescription
        }
    }

    private enum Flow {
        case qr
        case password
    }

    private func finish(_ success: LoginSuccess, cancelling other: Flow) async {
        guard !loggedIn else {
            return
        }
        loggedIn = true
        switch other {
        case .qr:
            stopQr()
            passwordLogin = nil
        case .password:
            passwordLogin?.cancel()
            passwordLogin = nil
            qrLogin = nil
        }
        await onLogin(success)
    }

    private func startQr() {
        qr = .connecting
        let flow: QrLogin
        do {
            flow = try client.qrLogin()
        } catch {
            qr = .failed(error as? LoginError ?? .UnexpectedResponse)
            return
        }
        qrLogin = flow
        qrLoop = Task { [weak self] in
            while true {
                let event: QrEvent
                do {
                    event = try await flow.next()
                } catch {
                    let error = error as? LoginError ?? .UnexpectedResponse
                    if error != .Cancelled, let self, self.qrLogin === flow {
                        self.qr = .failed(error)
                    }
                    return
                }
                guard let self else {
                    flow.cancel()
                    return
                }
                guard self.qrLogin === flow else {
                    return
                }
                switch event {
                case .code(let url):
                    self.qr = .code(url: url)
                case .scanned(let user):
                    self.qr = .scanned(user)
                case .cancelledOnPhone:
                    self.qr = .connecting
                case .captcha:
                    self.qr = .captchaUnsupported
                    self.stopQr()
                    return
                case .done(let success):
                    await self.finish(success, cancelling: .password)
                    return
                }
            }
        }
    }

    private func stopQr() {
        qrLogin?.cancel()
        qrLogin = nil
    }
}
