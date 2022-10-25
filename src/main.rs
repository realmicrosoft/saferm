use std::mem::MaybeUninit;
use std::os::unix::prelude::*;
use std::path::{Path, PathBuf};
use rce::*;

pub struct DeleteOptions {
    pub recursive: bool,
    pub umount: bool,
    pub dryrun: bool,
    pub allow_delete_above_start: bool,
    pub enter_symlinks: bool,
    pub verbose: bool,
    pub allow_hidden_files: bool,
    pub remove_symlinks: bool,
    pub starting_dir: PathBuf,
}

/// returns true if the path has something mounted
fn is_mountpoint(path: &Path) -> bool {
    use libc::*;
    let mut statbuf = MaybeUninit::<stat>::uninit();
    let mut statbuf2 = MaybeUninit::<stat>::uninit();
    let path_cstr = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let parent = path.parent();
    // if there isn't a parent, this is the root directory, and it's a mountpoint
    let parent_cstr = match parent {
        Some(p) => std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap(),
        None => return true,
    };
    unsafe {
        if lstat(path_cstr.as_ptr(), statbuf.as_mut_ptr()) != 0 {
            return false;
        }
        if lstat(parent_cstr.as_ptr(), statbuf2.as_mut_ptr()) != 0 {
            return false;
        }
        let statbuf = statbuf.assume_init();
        let statbuf2 = statbuf2.assume_init();
        statbuf.st_dev != statbuf2.st_dev
    }
}

fn delete(path: &str, options: &DeleteOptions) -> Result<(), ()> {
    let path = Path::new(path);
    // check if path is a symlink
    if path.is_symlink() {
        // if we are supposed to remove symlinks, remove it
        if options.remove_symlinks {
            println!("removing symlink {}", path.display());
            if !options.dryrun {
                std::fs::remove_file(path).unwrap();
            } else {
                println!("(dryrun) did nothing");
            }
            return Ok(());
        }
        if !options.enter_symlinks {
            println!("{} is a symlink, skipping", path.display());
            return Err(());
        }
    }
    // check if path is above starting dir
    if !options.allow_delete_above_start {
        if !path.canonicalize().unwrap().starts_with(&options.starting_dir) {
            println!("{} is above starting dir, skipping", path.display());
            return Err(());
        }
    }
    // check if this is a hidden file or directory
    if !options.allow_hidden_files {
        if path.file_name().unwrap_or("".as_ref()).to_str().unwrap_or("").starts_with(".") {
            println!("{} is a hidden file, skipping", path.display());
            return Err(());
        }
    }
    // check if path is a mount point
    if is_mountpoint(path) {
        if options.umount {
            println!("{} is a mount point, unmounting", path.display());
            if !options.dryrun {
                //umount(path).unwrap();
            } else {
                println!("(dryrun) did nothing");
            }
            return Ok(());
        } else {
            println!("{} is a mount point, skipping", path.display());
            return Err(());
        }
    }
    // check if path is a directory
    if path.is_dir() {
        if options.recursive {
            if options.verbose { println!("{} is a directory, recursing", path.display()); }
            for entry in std::fs::read_dir(path).unwrap() {
                // check if symlink
                let entry = entry.unwrap();
                let path = entry.path();
                let _ = delete(path.to_str().unwrap(), options);
            }
        } else {
            println!("{} is a directory, skipping", path.display());
            return Err(());
        }
    }
    // delete path
    if options.verbose { println!("deleting {}", path.display()); }
    if !options.dryrun {
        let res = std::fs::remove_file(path);
        if res.is_err() {
            println!("error deleting {}: {}", path.display(), res.unwrap_err());
        }
    } else if options.verbose { println!("(dryrun) did nothing"); }

    Ok(())
}

fn main() {
    let mut cmd = CommandInterface::new(
        "saferm",
        "a way to delete files with less worry of destroying your system",
    );

    let a_path = cmd.add_argument(Invoker::NWithoutInvoker(0), "path");
    let f_recursive = cmd.add_flag(
        Invoker::DashAndDoubleDash("r", "recursive"),
        "delete directories"
    );
    let f_umount = cmd.add_flag(
        Invoker::DashAndDoubleDash("u", "umount"),
        "unmount all found mount points"
    );
    let f_dryrun = cmd.add_flag(
        Invoker::DashAndDoubleDash("d", "dryrun"),
        "don't actually delete anything"
    );
    let f_allow_delete_above_start = cmd.add_flag(
        Invoker::DashAndDoubleDash("a", "allow-delete-above-start"),
        "allow deleting files above the directory specified"
    );
    let f_enter_symlinks = cmd.add_flag(
        Invoker::DashAndDoubleDash("s", "enter-symlinks"),
        "allow traversing symbolic links"
    );
    let f_verbose = cmd.add_flag(
        Invoker::DashAndDoubleDash("v", "verbose"),
        "print more information"
    );
    let f_allow_hidden_files = cmd.add_flag(
        Invoker::DashAndDoubleDash("h", "allow-hidden-files"),
        "allow deleting hidden files/folders"
    );
    let f_remove_symlinks = cmd.add_flag(
        Invoker::DashAndDoubleDash("rs", "remove-symlinks"),
        "remove symbolic links"
    );

    let f_help = cmd.add_flag(
        Invoker::DashAndDoubleDash("h", "help"),
        "display this help message"
    );

    let c_default = cmd.add_command(
        Invoker::Default,
        vec![a_path],
        "delete a file"
    );

    cmd.finalise();

    let input = cmd.go_and_print_usage_on_failure();
    if input.is_err() {
        return;
    }
    let input = input.unwrap();

    if input.flags.contains(&f_help) {
        cmd.print_help();
        return;
    }

    let path = input.inputs[0].clone();
    let recursive = input.flags.contains(&f_recursive);
    let umount = input.flags.contains(&f_umount);
    let dryrun = input.flags.contains(&f_dryrun);
    let allow_delete_above_start = input.flags.contains(&f_allow_delete_above_start);
    let enter_symlinks = input.flags.contains(&f_enter_symlinks);
    let verbose = input.flags.contains(&f_verbose);
    let allow_hidden_files = input.flags.contains(&f_allow_hidden_files);
    let remove_symlinks = input.flags.contains(&f_remove_symlinks);

    // get real path
    let path = Path::new(&path);
    //let path = path.canonicalize().unwrap();
    // if path doesn't start with /, get working dir and append it
    let path = if path.starts_with("/") {
        path.to_path_buf()
    } else {
        let mut path_a = std::env::current_dir().unwrap();
        path_a.push(path);
        path_a
    };
    let path = path.to_str().unwrap();

    let delete_options = DeleteOptions {
        recursive,
        umount,
        dryrun,
        allow_delete_above_start,
        enter_symlinks,
        verbose,
        allow_hidden_files,
        remove_symlinks,
        starting_dir: Path::new(&path).to_path_buf(),
    };

    // assert that the path is valid
    let exists = delete_options.starting_dir.try_exists();
    if exists.is_err() {
        println!("error: path is invalid");
        println!("  {}", exists.err().unwrap());
        return;
    }
    if !exists.unwrap() {
        println!("error: path does not exist");
        return;
    }

    let result = delete(path, &delete_options);
}
