package games.jvrcruz.siliconcraft.vm.network;

/** Constants shared with the N2 library. The emulator implements the port itself in Rust. */
public final class NetworkPort {
    public static final int IDLE = 0, PENDING = 1, OK = 2, DISABLED = -1, DENIED = -2, IO_ERROR = -3,
            LIMIT = -4, INVALID = -5, TIMEOUT = -6;
    public static final int TCP = 1, UDP = 2, SMTP = 3, SMTPS = 4, IMAP = 5, IMAPS = 6;
    public static final int HTTP = 7, HTTPS = 8, WS = 9, WSS = 10;
    public static final int MAX_TEXT = 2048;

    private NetworkPort() {}
}
