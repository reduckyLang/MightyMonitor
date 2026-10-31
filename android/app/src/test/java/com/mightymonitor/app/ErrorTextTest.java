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
    private static final String CONNECTION = "Couldn't connect. Make sure both devices are on the same Wi-Fi, then try again.";
    private static final String VIDEO = "The video could not be shown. Tap Connect to try again.";
    private static final String LINK = "This connection link isn't valid or has expired. Ask the other device for a new link.";
    private static final String MISSING = "This app is missing a part. Reinstall Trinix.";
    private static final String PERMISSION = "Screen sharing was not allowed. Tap Share, then Share this screen, and choose Start now.";
    // "link" is allowed only in the one message that is about the connection link itself.
    private static final String[] BANNED = {
        "IP", "port", "TCP", "UDP", "TLS", "SSL", "certificate", "fingerprint", "token", "codec",
        "H.264", "bitrate", "kbit", "fps", "session id", "handshake", "socket", "bind", "listener",
        "mDNS", "RTT", "ABR", "endpoint", "legacy", "unencrypted", "invitation", "payload", "protocol",
        "address", "runtime", "driver", "package", "portal", "Wayland", "PipeWire", "GStreamer",
        "SHA", "firewall", "VPN"
    };

    // Strings below are the real messages produced by the Java code, the Rust bridge and the security crate.
    private static final String[] LINK_INPUTS = {
        "Connection verification failed. Ask for a new sharing link.",
        "Invalid connection link",
        "Connection link is too long",
        "Paste a Trinix connection link",
        "Invalid sharing address",
        "Missing sharing address",
        "Missing host identity",
        "Use a specific sharing address and port",
        "Paste the connection link from the sharing device",
        "Enter a device IP:port or a connection link",
        "Invalid host identity",
        "Missing viewing permission",
        "Invalid viewing permission",
        "Host rejected the session"
    };
    private static final String[] VIDEO_INPUTS = {
        "Video arrived before configuration",
        "Unsupported network event",
        "Empty network event",
        "Incomplete H.264 access unit",
        "Invalid H.264 access unit",
        "Video access unit exceeds decoder capacity",
        "Invalid H.264 dimensions or frame rate",
        "Unexpected host message or unsupported video format"
    };
    private static final String[] CONNECTION_INPUTS = {
        "Could not start the network client",
        "Host handshake or video stream timed out",
        "Host disconnected",
        "Host ended the stream",
        "Connection refused",
        "No route to host"
    };

    @Test public void connectionLinkProblemsAskForANewLink() {
        for (String input : LINK_INPUTS) assertPlain(input, LINK);
    }

    @Test public void videoProblemsAskToTryAgain() {
        for (String input : VIDEO_INPUTS) assertPlain(input, VIDEO);
    }

    @Test public void networkProblemsGiveWifiAdvice() {
        for (String input : CONNECTION_INPUTS) assertPlain(input, CONNECTION);
    }

    @Test public void permissionProblemsExplainHowToAllow() {
        assertPlain("Screen sharing permission was not granted", PERMISSION);
    }

    @Test public void unknownAndNullMessagesUseFallback() {
        assertEquals(CONNECTION, ErrorText.plain("Something unexpected happened"));
        assertEquals(CONNECTION, ErrorText.plain((String) null));
        assertEquals(CONNECTION, ErrorText.plain(new IllegalStateException("Something unexpected happened")));
    }

    @Test public void exceptionTypesHavePlainMessages() {
        assertEquals(CONNECTION, ErrorText.plain(new UnknownHostException("raw host")));
        assertEquals(CONNECTION, ErrorText.plain(new ConnectException("raw connect")));
        assertEquals(CONNECTION, ErrorText.plain(new SocketTimeoutException("raw timeout")));
        assertEquals(MISSING, ErrorText.plain(new UnsatisfiedLinkError("raw native detail")));
        assertEquals(PERMISSION, ErrorText.plain(new SecurityException("raw security detail")));
        assertEquals(LINK, ErrorText.plain(new IllegalArgumentException("Invalid connection link")));
    }

    @Test public void throwableMessagesAreNeverReturned() {
        Throwable error = new IllegalArgumentException("sensitive invalid invitation details");
        assertFalse(ErrorText.plain(error).contains(error.getMessage()));
    }

    private static void assertPlain(String input, String expected) {
        String output = ErrorText.plain(input);
        assertEquals(input, expected, output);
        assertNotEquals(input, output);
        assertFalse(output.toLowerCase(Locale.ROOT).contains(input.toLowerCase(Locale.ROOT)));
        for (String banned : BANNED) {
            Pattern pattern = Pattern.compile(banned.equals("H.264") ? Pattern.quote(banned) : "\\b" + Pattern.quote(banned) + "\\b", Pattern.CASE_INSENSITIVE);
            assertFalse("Found banned word: " + banned, pattern.matcher(output).find());
        }
        assertTrue(output.endsWith("."));
    }
}
