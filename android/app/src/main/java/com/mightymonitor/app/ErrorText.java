package com.mightymonitor.app;

import java.net.ConnectException;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.util.Locale;

/** Maps raw error text to a plain message with one next step. Pure Java so it runs in JVM unit tests. */
final class ErrorText {
    private static final String CONNECTION = "Couldn't connect. Make sure both devices are on the same Wi-Fi, then try again.";
    private static final String VIDEO = "The video could not be shown. Tap Connect to try again.";
    private static final String LINK = "This connection link isn't valid or has expired. Ask the other device for a new link.";
    private static final String MISSING = "This app is missing a part. Reinstall Trinix.";
    private static final String PERMISSION = "Screen sharing was not allowed. Tap Share, then Share this screen, and choose Start now.";

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
        if (message.contains("connection verification failed")
                || message.contains("invalid connection link")
                || message.contains("connection link is too long")
                || message.contains("paste a trinix connection link")
                || message.contains("paste the connection link")
                || message.contains("missing sharing address")
                || message.contains("missing host identity")
                || message.contains("use a specific sharing address")
                || message.contains("enter a device ip:port or a connection link")
                || message.contains("invalid sharing address")
                || message.contains("invalid host identity")
                || message.contains("viewing permission")
                || message.contains("host rejected the session")
                || message.contains("invalid link")
                || message.contains("invalid invitation")
                || message.contains("authentication")
                || message.contains("pairing")
                || message.contains("expired")
                || message.matches(".*\\bpin\\b.*")) return LINK;
        if (message.contains("video arrived before configuration")
                || message.contains("unsupported network event")
                || message.contains("empty network event")
                || message.contains("incomplete h.264 access unit")
                || message.contains("invalid h.264 access unit")
                || message.contains("invalid h.264 dimensions")
                || message.contains("unsupported video format")
                || message.contains("video access unit exceeds decoder capacity")) return VIDEO;
        if (message.contains("native")) return MISSING;
        return CONNECTION;
    }
}
