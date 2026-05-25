use alloc::{string::String, vec::Vec};

use crate::{fs, ipc::pipe::Pipe, process, userspace};

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
        "/bin/cat",
        "cat /tmp/message.txt | cat",
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
        let output = run_command(left.trim(), cwd);
        let mut pipe = Pipe::new(256);
        pipe.write(output.as_bytes());
        let mut bytes = [0; 256];
        let read = pipe.read(&mut bytes);
        let piped_input = core::str::from_utf8(&bytes[..read]).unwrap_or("");
        let command = right.trim();
        if command == "cat" {
            crate::print!("{}", piped_input);
        } else {
            run_command(command, cwd);
        }
        return;
    }

    if let Some((command, target)) = line.split_once('>') {
        run_redirected(command.trim(), target.trim(), cwd);
        return;
    }

    run_command(line, cwd);
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
        "/bin/cat" => run_external("/bin/cat"),
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
    run_external_with_stdio(path, args, None)
}

fn run_external_with_redirect(path: &'static str, args: &[&str], stdout_path: &str) -> String {
    run_external_with_stdio(path, args, Some(stdout_path))
}

fn run_external_with_stdio(path: &'static str, args: &[&str], stdout_path: Option<&str>) -> String {
    let parent = 1;
    let child = process::fork(parent).expect("shell fork failed");
    process::exec(child, path);
    let mut argv = Vec::new();
    argv.push(path);
    argv.extend_from_slice(args);
    let result = userspace::run_user_program_with_stdio(path, &argv, child, stdout_path);
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
