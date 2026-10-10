import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct LoginModelTests {
    let client = FakeClient()
    let qr = FakeQrLogin()
    let password = FakePasswordLogin()
    let clock = Locked(Date(timeIntervalSince1970: 1_000_000))
    let logins = Locked<[UserId]>([])

    init() {
        client.qrLogins.withLock { $0 = [qr] }
        client.passwordLogins.withLock { $0 = [password] }
    }

    func makeModel() -> LoginModel {
        LoginModel(client: client, notice: nil, now: { [clock] in clock.current }) {
            [logins] success in
            logins.withLock { $0.append(success.userId) }
        }
    }

    func filledIn() -> LoginModel {
        let model = makeModel()
        model.login = "me@example.com"
        model.password = "hunter2"
        return model
    }

    @Test
    func qrStatesFollowTheEvents() async {
        let model = makeModel()
        let scanned = ScannedUser(id: id(42), username: "me", discriminator: "0", avatar: nil)

        model.appear()
        #expect(model.qr == .connecting)

        qr.send(.code(url: "https://discord.com/ra/first"))
        await qr.events.pulled(2)
        #expect(model.qr == .code(url: "https://discord.com/ra/first"))

        qr.send(.scanned(user: scanned))
        await qr.events.pulled(3)
        #expect(model.qr == .scanned(scanned))

        qr.send(.cancelledOnPhone)
        await qr.events.pulled(4)
        #expect(model.qr == .connecting)

        qr.send(.code(url: "https://discord.com/ra/second"))
        await qr.events.pulled(5)
        #expect(model.qr == .code(url: "https://discord.com/ra/second"))

        qr.send(.done(success: success(42)))
        await finished(model.qrLoop)
        #expect(logins.current == [id(42)])
    }

    @Test
    func aQrCaptchaShowsUnsupported() async {
        let model = makeModel()
        model.appear()

        qr.send(.captcha(challenge: captcha))
        await qr.cancelled.wait()

        #expect(model.qr == .captchaUnsupported)
    }

    @Test
    func aFailedQrLoginCanRestart() async {
        let second = FakeQrLogin()
        client.qrLogins.withLock { $0.append(second) }
        let model = makeModel()
        model.appear()

        qr.fail(.RemoteAuth(message: "the code expired"))
        await finished(model.qrLoop)
        #expect(model.qr == .failed(.RemoteAuth(message: "the code expired")))

        model.restartQr()
        #expect(model.qr == .connecting)
        second.send(.code(url: "https://discord.com/ra/again"))
        await second.events.pulled(2)
        #expect(model.qr == .code(url: "https://discord.com/ra/again"))

        client.qrError.withLock { $0 = .Network(kind: .connect) }
        model.restartQr()
        #expect(second.cancelled.fired)
        #expect(model.qr == .failed(.Network(kind: .connect)))
    }

    @Test
    func aPasswordLoginWithMfaLogsIn() async {
        let model = filledIn()
        password.reply(.mfa(challenge: mfa([.sms, .totp, .backup])))
        password.reply(.done(success: success(42)))

        await model.submit()
        #expect(model.step == .mfa(mfa([.sms, .totp, .backup]), method: .totp))
        model.code = "123456"
        await model.submit()

        #expect(
            password.calls.current == [
                .submit("me@example.com", "hunter2"), .submitMfa(.totp, "123456"),
            ]
        )
        #expect(logins.current == [id(42)])
        #expect(!model.isSubmitting)
    }

    @Test
    func smsSendsACodeFirst() async {
        let model = filledIn()
        password.reply(.mfa(challenge: mfa([.totp, .sms])))
        password.reply(.mfa(challenge: mfa([.totp, .sms], smsSentTo: "+1 ***-***-0123")))
        password.reply(.done(success: success(42)))
        await model.submit()

        await model.choose(.sms)
        #expect(model.step == .mfa(mfa([.totp, .sms], smsSentTo: "+1 ***-***-0123"), method: .sms))
        model.code = "222333"
        await model.submit()

        #expect(
            password.calls.current == [
                .submit("me@example.com", "hunter2"), .sendMfaSms, .submitMfa(.sms, "222333"),
            ]
        )
        #expect(logins.current == [id(42)])
    }

    @Test
    func choosingAnotherMethodSendsNothing() async {
        let model = filledIn()
        password.reply(.mfa(challenge: mfa([.totp, .backup])))
        await model.submit()

        await model.choose(.backup)

        #expect(model.step == .mfa(mfa([.totp, .backup]), method: .backup))
        #expect(password.calls.current == [.submit("me@example.com", "hunter2")])
    }

    @Test
    func aNewLocationIsConfirmedByLinkOrCode() async {
        let byEmail = filledIn()
        password.reply(.newLocation(via: .email))
        password.reply(.done(success: success(42)))
        await byEmail.submit()
        #expect(byEmail.step == .newLocation(.email))
        byEmail.code = "https://discord.com/authorize-ip#token=abc"
        await byEmail.submit()

        let phone = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [phone] }
        let byPhone = filledIn()
        phone.reply(.newLocation(via: .phone))
        phone.reply(.done(success: success(43)))
        await byPhone.submit()
        #expect(byPhone.step == .newLocation(.phone))
        byPhone.code = "445566"
        await byPhone.submit()

        #expect(
            password.calls.current == [
                .submit("me@example.com", "hunter2"),
                .confirmNewLocation("https://discord.com/authorize-ip#token=abc"),
            ]
        )
        #expect(phone.calls.current == [.submit("me@example.com", "hunter2"), .verifyPhone("445566")])
        #expect(logins.current == [id(42), id(43)])
    }

    @Test
    func invalidCredentialsShowDiscordsMessage() async {
        let model = filledIn()
        password.fail(.InvalidCredentials(message: "Login or password is invalid."))

        await model.submit()

        #expect(model.fieldError == "Login or password is invalid.")
        #expect(model.message == nil)
        #expect(model.step == .credentials)
    }

    @Test
    func aWrongCodeStaysOnTheMfaStep() async {
        let model = filledIn()
        password.reply(.mfa(challenge: mfa([.totp])))
        password.fail(.InvalidMfaCode)
        await model.submit()
        model.code = "000000"

        await model.submit()

        #expect(model.step == .mfa(mfa([.totp]), method: .totp))
        #expect(model.fieldError == LoginError.InvalidMfaCode.localizedDescription)
    }

    @Test(arguments: [
        LoginError.AccountDisabled, .AccountSuspended, .AccountScheduledForDeletion, .Blocked,
        .Network(kind: .connect), .RateLimited(retryAfter: nil, global: true),
    ])
    func otherErrorsShowAMessage(_ error: LoginError) async {
        let model = filledIn()
        password.fail(error)

        await model.submit()

        #expect(model.message == error.localizedDescription)
        #expect(model.fieldError == nil)
        #expect(model.rateLimitedUntil == nil)
    }

    @Test
    func anExpiredLoginStartsOver() async {
        let model = filledIn()
        password.reply(.mfa(challenge: mfa([.totp])))
        password.fail(.Expired)
        await model.submit()
        model.code = "123456"

        await model.submit()

        #expect(model.step == .credentials)
        #expect(model.message == LoginError.Expired.localizedDescription)
    }

    @Test
    func rateLimitsBlockSubmittingUntilTheyEnd() async {
        let model = filledIn()
        let start = clock.current
        password.fail(.RateLimited(retryAfter: 30, global: false))

        await model.submit()
        #expect(model.rateLimitedUntil == start.addingTimeInterval(30))
        #expect(!model.canSubmit(at: start.addingTimeInterval(29)))
        #expect(model.canSubmit(at: start.addingTimeInterval(30)))

        clock.withLock { $0 = start.addingTimeInterval(10) }
        await model.submit()
        #expect(password.calls.current.count == 1)

        clock.withLock { $0 = start.addingTimeInterval(31) }
        password.reply(.done(success: success(42)))
        await model.submit()
        #expect(password.calls.current.count == 2)
    }

    @Test
    func submittingNeedsTheFields() async {
        let model = makeModel()
        let now = clock.current

        #expect(!model.canSubmit(at: now))
        model.login = "me@example.com"
        #expect(!model.canSubmit(at: now))
        model.password = "hunter2"
        #expect(model.canSubmit(at: now))

        password.reply(.mfa(challenge: mfa([.totp])))
        await model.submit()
        #expect(!model.canSubmit(at: now))
        model.code = "1"
        #expect(model.canSubmit(at: now))
    }

    @Test
    func aCaptchaShowsUnsupportedAndKeepsTheForm() async {
        let model = filledIn()
        password.reply(.captcha(challenge: captcha))

        await model.submit()
        #expect(model.step == .captchaUnsupported)
        #expect(!model.canSubmit(at: clock.current))
        model.dismissCaptcha()

        #expect(model.step == .credentials)
        #expect(model.login == "me@example.com")
        #expect(model.password == "hunter2")
    }

    @Test
    func whicheverFlowFinishesFirstCancelsTheOther() async {
        let model = filledIn()
        model.appear()

        async let submitting: Void = model.submit()
        await password.replies.pulled(1)
        qr.send(.done(success: success(7)))
        await password.cancelled.wait()
        await submitting
        await finished(model.qrLoop)

        #expect(logins.current == [id(7)])
        #expect(model.message == nil)
        #expect(model.fieldError == nil)

        let other = makeModel()
        let second = FakeQrLogin()
        client.qrLogins.withLock { $0 = [second] }
        client.passwordLogins.withLock { $0 = [FakePasswordLogin()] }
        other.appear()
        other.login = "me@example.com"
        other.password = "hunter2"
        let flow = client.passwordLogins.current.first
        flow?.reply(.done(success: success(8)))
        await other.submit()
        await second.cancelled.wait()

        #expect(logins.current == [id(7), id(8)])
    }

    @Test
    func backToFormLeavesMfa() async {
        let model = filledIn()
        password.reply(.mfa(challenge: mfa([.totp])))
        await model.submit()
        model.code = "12"
        password.fail(.InvalidMfaCode)
        await model.submit()

        model.backToForm()

        #expect(model.step == .credentials)
        #expect(model.code.isEmpty)
        #expect(model.fieldError == nil)
        #expect(password.calls.current.last == .cancel)
        let second = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [second] }
        second.fail(.InvalidCredentials(message: "Login or password is invalid."))
        await model.submit()
        #expect(second.calls.current == [.submit("me@example.com", "hunter2")])
    }

    @Test
    func releasingTheModelCancelsItsLogins() async {
        var model: LoginModel? = filledIn()
        model?.appear()
        password.reply(.mfa(challenge: mfa([.totp])))
        await model?.submit()
        await qr.events.pulled(1)

        model = nil

        await qr.cancelled.wait()
        await password.cancelled.wait()
    }

    @Test
    func aStepThatEndsAfterBackToFormChangesNothing() async {
        let model = filledIn()
        async let submitting: Void = model.submit()
        await password.replies.pulled(1)

        password.reply(.mfa(challenge: mfa([.totp])))
        model.backToForm()
        await submitting
        #expect(model.step == .credentials)

        let second = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [second] }
        async let failing: Void = model.submit()
        await second.replies.pulled(1)
        second.fail(.InvalidCredentials(message: "Login or password is invalid."))
        model.backToForm()
        await failing

        #expect(model.fieldError == nil)
    }

    @Test
    func disappearCancelsBothFlows() async {
        let model = filledIn()
        model.appear()
        async let submitting: Void = model.submit()
        await password.replies.pulled(1)

        model.disappear()
        await submitting

        #expect(qr.cancelled.fired)
        #expect(password.cancelled.fired)
        #expect(model.message == nil)
        #expect(logins.current.isEmpty)
    }
}
