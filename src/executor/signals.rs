/// Returns a string with the text representation of the signal.
///
/// The values returned are based on the Linux x86 implementation:
/// https://github.com/torvalds/linux/blob/6f0d349d922ba44e4348a17a78ea51b7135965b1/arch/x86/include/uapi/asm/signal.h
pub fn strsignal(signal: u32) -> String {
    format!(
        "Signal {} ({})",
        signal,
        match signal {
            1 => "SIGHUP",
            2 => "SIGINT",
            3 => "SIGQUIT",
            4 => "SIGILL",
            5 => "SIGTRAP",
            6 => "SIGABRT",
            7 => "SIGBUS",
            8 => "SIGFPE",
            9 => "SIGKILL",
            10 => "SIGUSR1",
            11 => "SIGSEGV",
            12 => "SIGUSR2",
            13 => "SIGPIPE",
            14 => "SIGALRM",
            15 => "SIGTERM",
            16 => "SIGSTKFLT",
            17 => "SIGCHLD",
            18 => "SIGCONT",
            19 => "SIGSTOP",
            20 => "SIGTSTP",
            21 => "SIGTTIN",
            22 => "SIGTTOU",
            23 => "SIGURG",
            24 => "SIGXCPU",
            25 => "SIGXFSZ",
            26 => "SIGVTALRM",
            27 => "SIGPROF",
            28 => "SIGWINCH",
            29 => "SIGIO",
            30 => "SIGPWR",
            31 => "SIGSYS",
            34 => "SIGRTMIN",
            n @ 35..=49 => return format!("Signal {} (SIGRTMIN+{})", n, n - 34),
            n @ 50..=63 => return format!("Signal {} (SIGRTMAX-{})", n, n - 64),
            n => return format!("Signal {}", n),
        }
    )
}
