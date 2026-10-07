# Login screen

The first screen after launch when no account is stored. Discord users know it as one card
with two ways in, side by side. Akari keeps that layout so nobody has to look for the QR
code or the password form.

## Layout

A single centered card on a dark backdrop (light backdrop in the light theme). The window
can be resized; the card keeps its size and stays centered.

| Left column (wider) | Right column |
|---|---|
| A welcome-back heading and a one-line subtitle | A QR code in a rounded white tile |
| Field for the email address or phone number | Heading: log in with a QR code |
| Password field (hidden, with a reveal toggle) | One line: scan it with the Discord mobile app |
| Link for a forgotten password | |
| Primary log-in button, full width | |
| Link to register an account | |

Discord also offers a passkey link under the QR code. Akari leaves it out until it
supports WebAuthn.

Fields have their label above them, in small caps. Errors appear in the label line, in
red, next to the field they belong to, using Discord's message. The button shows
a spinner while a request runs and stays disabled until both fields have text.

When the window is too narrow for two columns, the QR column disappears and the form
fills the card, as in Discord.

## Both ways at once

The QR code and the form work at the same time. The QR login starts as soon as the screen
appears. Whichever finishes first wins, and the screen cancels the other one, so a code
scanned while the user is typing still logs them in.

QR code states:

| State | What the right column shows |
|---|---|
| Connecting | A placeholder tile with a spinner |
| Code | The code. It expires after a few minutes and is replaced on its own; the user never presses a refresh button |
| Scanned | The scanning user's avatar (default avatar if they have none) and name, a prompt to confirm on the phone, and a link to start over |
| Cancelled on the phone | Back to a fresh code |
| Failed | A short message and a retry button in place of the tile |

## Follow-up steps

**Captcha.** When Discord asks for a captcha, a sheet slides over the card: a macOS sheet,
a libadwaita dialog on Linux. It hosts a web view with the hCaptcha challenge. Solving it
closes the sheet and the login continues on its own. Cancelling closes the sheet, puts
the user back on the filled-in form, and lets them press the log-in button again.

**Two-factor authentication.** After a correct password on an account with MFA, the form
column switches to a code step. Its contents:

- Heading: two-factor authentication.
- A field for the code.
- Links to switch the method: authenticator app, SMS (sends the code, then shows the
  masked number), backup code, security key.
- A link back to the form.

The QR column stays as it is.

**New login location.** Discord can ask the user to confirm a login from a new place by
email or SMS. Discord's email link opens the official client, so Akari shows its own step
here: what happened, and a field to paste the address the link opened. For phone logins,
the step asks for the code from the SMS instead. Discord's client only tells the user to
check their email; the paste field exists because Akari can't receive the link.

## Behavior

- Return submits the focused step; Tab moves between fields in reading order.
- Escape leaves a follow-up step (MFA, new location) and returns to the form; on the form
  it does nothing.
- Rate limits show as a message above the button with the wait time when Discord gives
  one; the button stays disabled until then.
- A disabled, suspended or deleted account gets a plain message instead of field errors.
- After a successful login the screen fades into the main window. The token goes straight
  into the platform's secret store and never appears in the UI.

## Assets

No Discord logo, illustrations or the gg sans font. The card uses system fonts and
platform icons (SF Symbols on Apple, Adwaita icons on Linux); the QR tile is drawn by the
app.
