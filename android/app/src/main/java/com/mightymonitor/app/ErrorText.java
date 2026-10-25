package com.mightymonitor.app;

import java.net.ConnectException;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.util.Locale;

final class ErrorText {
    private static final String CONNECTION = "Couldn't connect. Make sure both devices are on the same Wi-Fi, then try again.";
    private static final String VIDEO = "The video could not be shown. Tap Connect to try again.";
    private static final String CODE = "This code has expired or doesn't match. Ask the other device for a new code.";
    private static final String MISSING = "This app is missing a part. Reinstall MightyMonitor.";
    private static final String PERMISSION = "Screen sharing was not allowed. Tap Show my screen and choose Start now.";

    private ErrorText() { }

    static String plain(Throwable error) {
        if (error == null) return CONNECTION;
        if (error instanceof UnsatisfiedLinkError) return MISSING;
        if (error instanceof SecurityException) return PERMISSION;
        if (error instanceof UnknownHostException || error instanceof ConnectException || error instanceof SocketTimeoutException) return CONNECTION;
        return plain(error.getMessage());
    }

    static String plain(String rawMessage) {
        if (rawMessage == null || rawMessage.trim().isEmpty()) return CONNECTION;
        String message = rawMessage.toLowerCase(Locale.ROOT);
        if (message.contains("screen sharing permission was not granted")) return PERMISSION;
        if (message.contains("video arrived before configuration")
                || message.contains("unsupported network event")
                || message.contains("empty network event")
                || message.contains("incomplete h.264 access unit")
                || message.contains("invalid h.264 access unit")
                || message.contains("video access unit exceeds decoder capacity")) return VIDEO;
        if (message.matches(".*\\bpin\\b.*") || message.contains("authentication") || message.contains("pairing")
                || message.contains("expired") || message.contains("invalid invitation") || message.contains("invalid link")) return CODE;
        if (message.contains("native")) return MISSING;
        if (message.contains("timed out") || message.contains("timeout") || message.contains("refused")
                || message.contains("unreachable") || message.contains("no route")
                || message.contains("connection reset") || message.contains("broken pipe")) return CONNECTION;
        return CONNECTION;
    }
}
