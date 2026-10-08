/// Discord wants a CAPTCHA solved before it accepts the request.
///
/// Render the challenge (hCaptcha for login) in a web view with `sitekey` and, when
/// present, `rqdata`, then pass the solution to the flow's `solve_captcha`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptchaChallenge {
    /// `hcaptcha`, or `recaptcha_enterprise` for some endpoints.
    pub service: String,
    pub sitekey: Option<String>,
    /// Must be passed to the challenge if present, or the solution is rejected.
    pub rqdata: Option<String>,
    pub rqtoken: Option<String>,
    pub session_id: Option<String>,
    pub should_serve_invisible: bool,
}
