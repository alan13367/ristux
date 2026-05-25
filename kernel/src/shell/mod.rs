use alloc::{string::String, vec::Vec};

use crate::{fs, process, userspace};

pub fn init() {
    run_script(&[
        "help",
        "/bin/pwd",
        "echo hello from shell",
        "/bin/echo hello from argv",
        "/bin/ls /bin",
        "cat /tmp/message.txt",
        "/bin/echo redirected > /tmp/message.txt",
        "cat /tmp/message.txt",
        "/bin/cat < /tmp/message.txt",
        "/bin/cat /tmp/message.txt",
        "/bin/cat /tmp/message.txt | /bin/cat",
        "true",
        "false",
    ]);
    crate::println!("Shell self-test script completed.");
}

fn run_script(lines: &[&str]) {
    let mut cwd = String::from("/");
    for line in lines {
        run_line(line, &mut cwd);
    }
}

fn run_line(line: &str, cwd: &mut String) {
    crate::println!("sh$ {}", line);

    if let Some((left, right)) = line.split_once('|') {
        run_pipeline(left.trim(), right.trim(), cwd);
        return;
    }

    if let Some((command, target)) = line.split_once('>') {
        run_redirected(command.trim(), target.trim(), cwd);
        return;
    }

    if let Some((command, source)) = line.split_once('<') {
        run_input_redirected(command.trim(), source.trim(), cwd);
        return;
    }

    run_command(line, cwd);
}

fn run_pipeline(left: &str, right: &str, cwd: &mut String) {
    if run_external_pipeline(left, right) {
        return;
    }

    let output = run_command(left, cwd);
    if right == "cat" {
        crate::print!("{}", output);
    } else {
        run_command(right, cwd);
    }
}

fn run_external_pipeline(left: &str, right: &str) -> bool {
    let Some((left_path, left_args)) = parse_external_command(left) else {
        return false;
    };
    let Some((right_path, right_args)) = parse_external_command(right) else {
        return false;
    };

    let (read_fd, write_fd) = fs::create_pipe(4096).expect("shell pipe creation failed");
    run_external_with_fds(left_path, &left_args, None, Some(write_fd));
    run_external_with_fds(right_path, &right_args, Some(read_fd), None);
    crate::println!(
        "Ring 3 pipeline connected {} -> {} through VFS pipe.",
        left_path,
        right_path
    );
    true
}

fn parse_external_command(command: &str) -> Option<(&'static str, Vec<&str>)> {
    let mut parts = command.split_whitespace();
    let program = parts.next()?;
    let path = external_path(program)?;
    Some((path, parts.collect()))
}

fn external_path(program: &str) -> Option<&'static str> {
    match program {
        "cat" | "/bin/cat" => Some("/bin/cat"),
        "/bin/echo" => Some("/bin/echo"),
        "ls" | "/bin/ls" => Some("/bin/ls"),
        "pwd" | "/bin/pwd" => Some("/bin/pwd"),
        "true" | "/bin/true" => Some("/bin/true"),
        "false" | "/bin/false" => Some("/bin/false"),
        _ => None,
    }
}

fn run_redirected(command: &str, target: &str, cwd: &mut String) {
    let mut parts = command.split_whitespace();
    let Some(program) = parts.next() else {
        fs::write_file(target, b"");
        return;
    };
    let args: Vec<&str> = parts.collect();

    if program == "/bin/echo" {
        run_external_with_redirect("/bin/echo", &args, target);
        return;
    }

    let output = run_command(command, cwd);
    fs::write_file(target, output.as_bytes());
}

fn run_input_redirected(command: &str, source: &str, cwd: &mut String) {
    if let Some((path, args)) = parse_external_command(command) {
        let input_fd = fs::open(source).expect("shell input redirection open failed");
        run_external_with_fds(path, &args, Some(input_fd), None);
        crate::println!(
            "Ring 3 input redirection mapped {} to {} stdin.",
            source,
            path
        );
        return;
    }

    if command == "cat" {
        match fs::read_file(source) {
            Some(bytes) => match core::str::from_utf8(&bytes) {
                Ok(text) => {
                    crate::print!("{}", text);
                }
                Err(_) => {
                    crate::println!("<binary>");
                }
            },
            None => {
                crate::println!("cat: not found");
            }
        }
    } else {
        run_command(command, cwd);
    }
}

fn run_command(command: &str, cwd: &mut String) -> String {
    let mut parts = command.split_whitespace();
    let Some(program) = parts.next() else {
        return String::new();
    };
    let args: Vec<&str> = parts.collect();

    match program {
        "help" => output("builtins: help clear echo pwd cd exit ls cat true false\n"),
        "clear" => output("\x0c"),
        "echo" => {
            let mut text = args.join(" ");
            text.push('\n');
            output(&text)
        }
        "pwd" => run_external("/bin/pwd"),
        "cd" => {
            *cwd = String::from(args.first().copied().unwrap_or("/"));
            String::new()
        }
        "exit" => output("exit\n"),
        "ls" => run_external_with_args("/bin/ls", &args),
        "cat" => {
            let Some(path) = args.first() else {
                return String::new();
            };
            match fs::read_file(path) {
                Some(bytes) => match core::str::from_utf8(&bytes) {
                    Ok(text) => output(text),
                    Err(_) => output("<binary>\n"),
                },
                None => output("cat: not found\n"),
            }
        }
        "true" => run_external("/bin/true"),
        "false" => run_external("/bin/false"),
        "/bin/cat" => run_external_with_args("/bin/cat", &args),
        "/bin/echo" => run_external_with_args("/bin/echo", &args),
        "/bin/ls" => run_external_with_args("/bin/ls", &args),
        "/bin/pwd" => run_external("/bin/pwd"),
        other => {
            let mut text = String::from(other);
            text.push_str(": not found\n");
            output(&text)
        }
    }
}

fn output(text: &str) -> String {
    crate::print!("{}", text);
    String::from(text)
}

fn run_external(path: &'static str) -> String {
    run_external_with_args(path, &[])
}

fn run_external_with_args(path: &'static str, args: &[&str]) -> String {
    run_external_with_fds(path, args, None, None)
}

fn run_external_with_redirect(path: &'static str, args: &[&str], stdout_path: &str) -> String {
    run_external_with_stdio(path, args, Some(stdout_path))
}

fn run_external_with_stdio(path: &'static str, args: &[&str], stdout_path: Option<&str>) -> String {
    run_external_with_process(path, args, |argv, child| {
        userspace::run_user_program_with_stdio(path, argv, child, stdout_path)
    })
}

fn run_external_with_fds(
    path: &'static str,
    args: &[&str],
    stdin_vfs_fd: Option<usize>,
    stdout_vfs_fd: Option<usize>,
) -> String {
    run_external_with_process(path, args, |argv, child| {
        userspace::run_user_program_with_fds(path, argv, child, stdin_vfs_fd, stdout_vfs_fd)
    })
}

fn run_external_with_process(
    path: &'static str,
    args: &[&str],
    runner: impl FnOnce(&[&str], u64) -> userspace::UserProgramResult,
) -> String {
    let parent = 1;
    let child = process::fork(parent).expect("shell fork failed");
    process::exec(child, path);
    let mut argv = Vec::new();
    argv.push(path);
    argv.extend_from_slice(args);
    let result = runner(&argv, child);
    process::exit(child, result.status);
    let waited = process::wait(parent, child).unwrap_or(-1);
    crate::println!(
        "{} exited with {} after ring 3 exec ({} page(s) unmapped)",
        path,
        waited,
        result.unmapped_pages
    );
    String::new()
}
