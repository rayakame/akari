import Testing

@testable import AkariKit

struct ConnectionNoticeTests {
    @Test
    func noticesSayWhatHappened() {
        #expect(ConnectionNotice.connecting.text == "Connecting to Discord…")
        #expect(ConnectionNotice.reconnecting.text == "Connection lost. Reconnecting…")
        #expect(ConnectionNotice.offline.text == "Offline")
        #expect(ConnectionNotice.closed(nil).text == "Akari lost its connection to Discord.")
        #expect(
            ConnectionNotice.closed(.Rejected(code: 4013)).text
                == "Akari lost its connection to Discord (Discord refused the connection (4013))."
        )
    }

    @Test
    func onlyConnectingWaits() {
        #expect(ConnectionNotice.connecting.delay == 1)
        #expect(ConnectionNotice.reconnecting.delay == 1)
        #expect(ConnectionNotice.offline.delay == 0)
        #expect(ConnectionNotice.closed(.Stopped).delay == 0)
    }

    @Test
    func onlyAClosedSessionOffersReconnect() {
        #expect(!ConnectionNotice.connecting.offersReconnect)
        #expect(!ConnectionNotice.reconnecting.offersReconnect)
        #expect(!ConnectionNotice.offline.offersReconnect)
        #expect(ConnectionNotice.closed(nil).offersReconnect)
    }
}
