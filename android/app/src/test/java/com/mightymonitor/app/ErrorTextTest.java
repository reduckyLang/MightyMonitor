package com.mightymonitor.app;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertTrue;

import java.net.ConnectException;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.util.Locale;
import java.util.regex.Pattern;
import org.junit.Test;

public final class ErrorTextTest {
    private static final String FALLBACK = "Couldn't connect. Make sure both devices are on the same Wi-Fi, then try again.";
    private static final String[] BANNED = {
        "IP", "port", "TCP", "UDP", "TLS", "SSL", "certificate", "fingerprint", "token", "codec",
        "H.264", "bitrate", "kbit", "fps", "session id", "handshake", "socket", "bind", "listener",
        "mDNS", "RTT", "ABR", "endpoint", "legacy", "unencrypted", "invitation", "payload", "protocol",
        "link", "address", "runtime", "driver", "package", "portal", "Wayland", "PipeWire", "GStreamer",
        "SHA", "firewall", "VPN"
    };

    @Test public void knownMessagesArePlainAndDoNotEchoInput() {
        String[] inputs = {
            "Screen sharing permission was not granted",
            "Could not start the network client",
            "Video arrived before configuration",
            "Unsupported network event",
            "Empty network event",
            "Incomplete H.264 access unit",
            "Invalid H.264 access unit",
            "Video access unit exceeds decoder capacity",
            "Request timed out",
            "Connection timeout",
            "Connection refused",
            "Host unreachable",
            "No route to host",
            "Connection reset",
            "Broken pipe",
            "Certificate pin mismatch",
            "Authentication failed",
            "Pairing failed",
            "Code expired",
            "Invalid invitation",
            "Invalid link",
            "Native library unavailable"
        };
        for (String input : inputs) {
            String output = ErrorText.plain(input);
            assertFalse(output.isEmpty());
            assertNotEquals(input, output);
            assertFalse(output.toLowerCase(Locale.ROOT).contains(input.toLowerCase(Locale.ROOT)));
            assertNoBannedWords(output);
        }
    }

    @Test public void unknownAndNullMessagesUseFallback() {
        assertEquals(FALLBACK, ErrorText.plain("Something unexpected happened"));
        assertEquals(FALLBACK, ErrorText.plain((String) null));
        assertEquals(FALLBACK, ErrorText.plain(new IllegalStateException("Something unexpected happened")));
    }

    @Test public void exceptionTypesHavePlainMessages() {
        assertEquals(FALLBACK, ErrorText.plain(new UnknownHostException("raw host")));
        assertEquals(FALLBACK, ErrorText.plain(new ConnectException("raw connect")));
        assertEquals(FALLBACK, ErrorText.plain(new SocketTimeoutException("raw timeout")));
        assertEquals("This app is missing a part. Reinstall MightyMonitor.", ErrorText.plain(new UnsatisfiedLinkError("raw native detail")));
        assertEquals("Screen sharing was not allowed. Tap Show my screen and choose Start now.", ErrorText.plain(new SecurityException("raw security detail")));
    }

    @Test public void throwableMessagesAreNeverReturned() {
        Throwable error = new IllegalArgumentException("sensitive invalid invitation details");
        assertFalse(ErrorText.plain(error).contains(error.getMessage()));
    }

    private static void assertNoBannedWords(String output) {
        for (String banned : BANNED) {
            Pattern pattern = Pattern.compile(banned.equals("H.264") ? Pattern.quote(banned) : "\\b" + Pattern.quote(banned) + "\\b", Pattern.CASE_INSENSITIVE);
            assertFalse("Found banned word: " + banned, pattern.matcher(output).find());
        }
        assertTrue(output.endsWith("."));
    }
}
