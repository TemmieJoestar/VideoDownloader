use std::fmt::format;
use std::fs;
use std::fs::{remove_file, File};
use std::io;
use std::io::ErrorKind::ConnectionRefused;
use std::io::Write;
use std::option;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::copy_nonoverlapping;
use std::thread;
use std::time::Duration;

use colored::*;
use rfd::FileDialog;
use crate::PathError::{NotDirectory, NotWritable};

const MAX_SIZE_BYTES: u64 = 8 * 1024 * 1024;

enum UserPath {
    Separated {
        video_path: PathBuf,
        music_path: PathBuf,
    },
    Combined(PathBuf),
}

enum PathChoice {
    Separated,  // Two folder, one for Video, one for Music
    Combined,   // One folder for both Video and Music
}

enum PathSelection {
    Selected(PathBuf),  // When the user has chosen a "good" path. (Good means that the program can write inside that path)
    Cancelled,          // When the user has canceled the dialog
    Invalid(PathError), // When the path is invalid, see its Enum for more information
}

enum PathError {
    Empty,          // When `pick_folder()` returns an empty path
    NotWritable,    // When the program cannot write inside the given path
    NotDirectory,   // When then the given path is not a directory
}
#[derive(Debug)]
enum FileTestStage {
    Creating,
    Deleting,
}
#[derive(Debug)]
enum FileTestStatus {
    Success,
    Problem {
        stage: FileTestStage,
        description: String,
    },
}

struct WriteCheckResult {
    status: FileTestStatus,
    is_writable: bool,
}

fn main() {
    println!("hello world");
}
fn is_music(url: &str) -> bool {
    println!("{}", "Checking category...".cyan());
    let check_music = Command::new("yt-dlp")
        .args(["--print", "%(categories)s", url])
        .output()
        .expect("Failed to run yt-dlp");

    let categories_str = String::from_utf8_lossy(&check_music.stdout);
    let music_keywords= ["Music", "music", "Song", "song"];

    let is_music_cat = music_keywords
        .iter()
        .any(|keyword| categories_str.contains(keyword));

    println!("{}", format!("Category - Music: {}", is_music_cat).bold());
    is_music_cat
}

fn video_download(url: String) {
    let mut download = Command::new("yt-dlp");

    let output_dir: &str = if is_music(&url) {
        r"C:\Users\Temmie\Pictures\Pictures\Music\%(title)s.%(ext)s"
    } else {
        r"C:\Users\Temmie\Pictures\Pictures\Vidéo\%(title)s.%(ext)s"
    };

    println!("{}", "Downloading video...".cyan());

    download.args([
        "--encoding",
        "utf-8",
        "-o",
        output_dir,
        "--quiet",
        "--no-warnings",
        "--replace-in-metadata",
        "title",
        " ",
        "_",
        "--format-sort",
        "res:480,ext:mp4:m4a",
        "--concurrent-fragments",
        "5",
        "--extractor-args",
        "youtube:player_client=tv,mweb",
        "--print",
        "after_move:filepath",
        url.trim(),
    ]);

    let output = download
        .output()
        .unwrap_or_else(|_| panic!("{}", "Failed to download the video".bold().red()));
    let downloaded_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if let Ok(metadata) = fs::metadata(&downloaded_path) {
        let file_size = metadata.len();
        let size_mb = file_size as f64 / (1024.0 * 1024.0);

        if file_size <= MAX_SIZE_BYTES {
            println!("{}",format!("Downloaded size ~{:.2} MB (Target met!)", size_mb).bold().green());
            println!("{}", "Would you like to open Explorer? (y/N)".bold().cyan());
            let mut user_input = String::new();
            if let Err(err) = io::stdin().read_line(&mut user_input) {
                println!("{}",format!("Couldn't read your input: {err}").bold().red());
            }
            open_explorer(&downloaded_path, &user_input);
        } else {
            println!("{}",format!("Original size ~{:.2} MB is over 8 MB. Starting compression...",size_mb).yellow());
            compress_video(downloaded_path);
        }
    } else {
        eprintln!("{}", "Could not retrieve downloaded file metadata.".bold().red());
    }
}

fn compress_video(full_path: String) {
    let path: &Path = Path::new(&full_path);

    // Extracts the file name with its extension (e.g., "/path/to/example.mp4" -> "example.mp4").
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    let clean_file_name = format!("{}_compressed.mp4", stem);

    let target_dir = if full_path.contains("Music") {
        r"C:\Users\Temmie\Pictures\Pictures\Music"
    } else {
        r"C:\Users\Temmie\Pictures\Pictures\Vidéo"
    };

    let final_destination = std::path::PathBuf::from(target_dir).join(clean_file_name);

    let crf_values: [&str; 6] = ["28", "32", "36", "40", "44", "50"];

    for (index, crf) in crf_values.iter().enumerate() {
        let temp_output: String = format!(r"C:\Users\Temmie\Pictures\Pictures\Vidéo\temp\output_{}.mp4", index + 1);

        println!("{}", format!("Pass {}/{} — Testing CRF {}", index + 1, crf_values.len(), crf).cyan());

        let compress = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &full_path,
                "-c:v",
                "libx264",
                "-crf",
                crf,
                "-preset",
                "medium",
                "-c:a",
                "copy",
                "-movflags",
                "+faststart",
                &temp_output,
            ])
            .output()
            .unwrap_or_else(|_| panic!("{}", "Failed to run FFMPEG".bold().red()));

        if compress.status.success() {
            println!("{}", "FFMPEG pass completed!".green());

            if let Ok(metadata) = fs::metadata(&temp_output) {
                let file_size = metadata.len();
                let size_mb = file_size as f64 / (1024.0 * 1024.0);
                let is_last_pass = index + 1 == crf_values.len();

                // Condition 1: File fits under the size limit!
                if file_size <= MAX_SIZE_BYTES {
                    println!("{}", format!("Compressed size ~{:.2} MB (Target met!)", size_mb).bold().green());

                    let mut user_input = String::new();

                    if let Err(e) = fs::rename(&temp_output, &final_destination) {
                        eprintln!("{}", format!("Failed to rename compressed file: {}", e).bold().red());

                        println!("{}", "Would you like to open Explorer to view the temporary file? (Y/N)".bold().cyan());

                        if let Err(err) = io::stdin().read_line(&mut user_input) {
                            eprintln!("{}", format!("Couldn't read your input: {err}").bold().red());
                        }

                        open_explorer(&temp_output, &user_input);
                    } else {
                        if let Err(e) = fs::remove_file(&full_path) {
                            eprintln!("{}", format!("Failed to delete downloaded file: {}", e).bold().red());
                        }

                        println!("{}", "Would you like to open Explorer? (Y/N)".bold().cyan());

                        if let Err(err) = io::stdin().read_line(&mut user_input) {
                            eprintln!("{}", format!("Couldn't read your input: {err}").bold().red());
                        }

                        open_explorer(&final_destination, &user_input);
                    }

                    break;
                }
                // Condition 2: File is >8MB, BUT we reached our pass limit!
                else if is_last_pass {
                    println!("{}", format!("Loop limit reached. File size: ~{:.2} MB. Saving best attempt.", size_mb).bold().yellow());

                    let mut user_input = String::new();

                    if let Err(e) = fs::rename(&temp_output, &final_destination) {
                        eprintln!("{}", format!("Failed to rename compressed file: {}", e).bold().red());

                        println!("{}", "Would you like to open Explorer to view the temporary file? (Y/N)".bold().cyan());

                        if let Err(err) = io::stdin().read_line(&mut user_input) {
                            eprintln!("{}", format!("Couldn't read your input: {err}").bold().red());
                        }

                        open_explorer(&temp_output, &user_input);
                    } else {
                        if let Err(e) = fs::remove_file(&full_path) {
                            eprintln!("{}", format!("Failed to delete downloaded file: {}", e).bold().red());
                        }

                        println!("{}", "Would you like to open Explorer to view the saved file? (Y/N)".bold().cyan());

                        if let Err(err) = io::stdin().read_line(&mut user_input) {
                            eprintln!("{}", format!("Couldn't read your input: {err}").bold().red());
                        }

                        open_explorer(&final_destination, &user_input);
                    }

                    break;
                }
                // Condition 3: File is >8MB, and we still have more CRF passes to try.
                else {
                    println!("{}", format!("File size ~{:.2} MB is too large. Retrying with higher CRF...", size_mb).yellow());

                    // Clean up intermediate attempt
                    if let Err(e) = fs::remove_file(&temp_output) {
                        eprintln!("{}", format!("Failed to delete temp file: {}", e).bold().red());
                    }
                }
            }
        }
    }
}

fn open_explorer(target_path: impl AsRef<Path>, input: &str) {
    if input.trim().eq_ignore_ascii_case("y") {
        let path_str = target_path.as_ref().to_string_lossy();
        let clean_path = path_str.replace('/', r"\");

        if let Err(e) = Command::new("cmd")
            .args(["/c", "explorer", "/select,", &clean_path])
            .spawn()
        {
            eprintln!("{}", format!("Failed to open explorer: {e}").bold().red());
        }
    }
}

fn ask_path() {
    // Will be used to define both Music and Video path for the user.
    // It doesn't matter if both path are the same.
    // If each path are different it shall return a Vec![] that contains both path.
    // If both path are the same it shall return a Vec![] that contains that single path.
    // Example path : r"Path/To/Video/" <- This is a &'static (string slice)

    // Option 2:
    // Shows a windows like explorer, where the user navigate to its desirated path, or type it on the bottom bar.
    // This would prevent any path that doesn't exist, and path that aren't allowed.
    // It's also more user friendly than having it typing the path where that user could make mistake.

    println!("{}", "Welcome! It's your first time launching this program. Could you tell us where you want your files to be downloaded.".bold());
    println!("{}", "Do you want to have separated path for Music & Video ? (Y/N)".bold().cyan());
    let mut user_input: String = String::new();

    let choice: PathChoice = loop {
        if let Err(err) = io::stdin().read_line(&mut user_input) {
            eprint!("{}", format!("Couldn't read your input : {err}").bold().red())
        }

        match user_input.to_uppercase().trim() {
            "Y" => break PathChoice::Separated,
            "N" => break PathChoice::Combined,
            _ => println!("{}", "You must choose between Y and N".bold().yellow()),
        }
    };

    match choice {
        PathChoice::Separated => {
            println!("Separated");
            let video_dialog = FileDialog::new()
                .set_title("Choose your Video folder.")
                .set_directory("/")
                .pick_folder();

            if let Some(video_path) = video_dialog {
                println!(
                    "{}",
                    format!("Selected Video path : {}", video_path.display())
                        .bold()
                        .cyan()
                );
            } else {
                println!("{}", "User cancelled dialog.".bold().red());
            }

            let music_dialog = FileDialog::new()
                .set_title("Choose your Music folder.")
                .set_directory("/")
                .pick_folder();

            if let Some(music_path) = music_dialog {
                println!(
                    "{}",
                    format!("Selected Music path : {}", music_path.display())
                        .bold()
                        .cyan()
                );
            } else {
                println!("{}", "User cancelled dialog.".bold().red());
            }
        }
        PathChoice::Combined => {
            println!("Combined");
            let downloads_dialog = FileDialog::new()
                .set_title("Choose your Media folder.")
                .set_directory("/")
                .pick_folder();

            if let Some(downloads_path) = downloads_dialog {
                println!(
                    "{}",
                    format!("Selected Media path : {}", downloads_path.display())
                        .bold()
                        .cyan()
                );
            } else {
                println!("{}", "User cancelled dialog.".bold().red());
            }
        }
    }
    // return struct containing two or one path
}

fn select_separated() -> Option<UserPath> {
    let mut video_path = loop {
        match ask_video_path() {
            PathSelection::Selected(path) => break path,
            PathSelection::Cancelled => {
                if ask_confirmation("You've cancelled the Video dialog. Would you like to use a combined path? (Y/N)", ) {
                    return select_combined();
                }
                continue;
            }
            PathSelection::Invalid => {
                println!("{}", "Path is invalid.".bold().red());
                continue;
            }
        }
    };

    let mut music_path = loop {
        match ask_music_path() {
            PathSelection::Selected(path) => break path,
            PathSelection::Cancelled => {
                if ask_confirmation("You've cancelled the Music dialog. Would you like to use a combined path? (Y/N)",) {
                    return select_combined();
                }
                continue;
            }
            PathSelection::Invalid => {
                println!("{}", "Path is invalid.".bold().red());
                continue;
            }
        }
    };

    'confirmation: loop {
        println!("{}", format!("Video path: {}", video_path.display()).bold().yellow());
        println!("{}", format!("Music path: {}", music_path.display()).bold().yellow());
        if ask_confirmation("Are those path correct ? (Y/N)") {
            return Some(UserPath::Separated {video_path, music_path,});
        } else {
            'reselect_path: loop {
                println!("{}", "Which one would you like to re-select?".bold().bright_blue());
                println!("  {} Video", "[1]".bold().bright_cyan());
                println!("  {} Music", "[2]".bold().bright_cyan());
                let user_input = read_input();
                match user_input.trim() {
                    "1" => {
                        video_path = match ask_video_path() {
                            PathSelection::Selected(path) => path,
                            PathSelection::Cancelled => {
                                if ask_confirmation("You've cancelled the Video's dialog. Would you like to have combined path ? (Y/N)") {
                                    return select_combined();
                                }
                                continue 'reselect_path;
                            }
                            PathSelection::Invalid => {
                                println!("{}", "Path is invalid.".bold().red());
                                return None;
                            }
                        };
                    }
                    "2" => {
                        music_path = match ask_music_path() {
                            PathSelection::Selected(path) => path,
                            PathSelection::Cancelled => {
                                if ask_confirmation("You've cancelled Music's dialog. Would you like to have combined path ? (Y/N)") {
                                    return select_combined();
                                }
                                continue 'reselect_path;
                            }
                            PathSelection::Invalid => {
                                println!("{}", "Path is invalid.".bold().red());
                                return None;
                            }
                        };
                    }
                    _ => {
                        continue 'reselect_path;
                    }
                };
                continue 'confirmation;
            }
        }
    }
}

fn select_combined() -> Option<UserPath> {
    loop {
        let media_path = match ask_media_path() {
            PathSelection::Selected(path) => path,
            PathSelection::Cancelled => {
                println!("{}", "You've cancelled the Media dialog.".bold().yellow());
                if ask_confirmation("Would you like to re-enter your path? (Y/N)") {
                    continue;
                }
                return None;
            }
            PathSelection::Invalid => {
                println!("{}", "Path is invalid.".bold().red());
                return None;
            }
        };
        return Some(UserPath::Combined(media_path));
    }
}

fn rand_num_str() -> String {
    let mut numbers: String = String::new();
    for _ in 1..=10 {
        let random: i32 = rand::random_range(1..=9);
        numbers.push_str(&random.to_string());
    }
    numbers

}

fn can_write(user_path: &PathBuf) -> WriteCheckResult {
    let clean_path = user_path.join(format!("test_{}", rand_num_str()));
    if let Err(error) = File::create(&clean_path) {
        return WriteCheckResult {
            status: FileTestStatus::Problem {
                stage: FileTestStage::Creating,
                description: format!("Couldn't create the test file. Error details: {error} // Path: {}", clean_path.display()),
            },
            is_writable: false,
        };
    }

    println!("{}", format!("Successfully written the file in {}",user_path.display()).bold().bright_cyan());
    if let Err(error) = remove_file(&clean_path) {
        return WriteCheckResult {
            status: FileTestStatus::Problem {
                stage: FileTestStage::Deleting,
                description: format!("Couldn't delete the test file. Error details: {error} // Path: {}",clean_path.display()),
            },
            is_writable: true,
        };
    }
    WriteCheckResult {
        status: FileTestStatus::Success,
        is_writable: true,
    }
}

fn prompt_for_path(message: &str) -> PathSelection {
    println!("{}", format!("{message}").bold().cyan());
    let selected_path = FileDialog::new()
        .set_title(message)
        .set_directory("/")
        .pick_folder();

    let path = match selected_path {
        Some(path) => path,
        None => return PathSelection::Cancelled,
    };

    match can_write(&path).status {
        FileTestStatus::Success => {
            PathSelection::Selected(path)
        },
        FileTestStatus::Problem {stage: FileTestStage::Creating, description} => {
            PathSelection::Invalid(NotWritable)
        },
        FileTestStatus::Problem {stage: FileTestStage::Deleting, description} => {
            PathSelection::Selected(path)
        }
    }

}
/*
enum PathSelection {
    Selected(PathBuf),  // When the user has chosen a "good" path. (Good means that the program can write inside that path)
    Cancelled,          // When the user has canceled the dialog
    Invalid(PathError), // When the path is invalid, see its Enum for more information
}

enum PathError {
    Empty,          // When `pick_folder()` returns an empty path
    NotWritable,    // When the program cannot write inside the given path
    NotDirectory,   // When then the given path is not a directory
}
*/
fn ask_media_path() -> PathSelection {
    let media_path = match prompt_for_path("Choose your Media path.") {
        PathSelection::Selected(path) => path,
        PathSelection::Cancelled => return PathSelection::Cancelled,
        PathSelection::Invalid(NotWritable) => return PathSelection::Invalid(NotWritable),
        PathSelection::Invalid(PathError::Empty) => return PathSelection::Invalid(PathError::Empty),
        PathSelection::Invalid(NotDirectory) => return PathSelection::Invalid(NotDirectory),
    };
    PathSelection::Selected(media_path)
}
fn ask_music_path() -> PathSelection {
    let music_path = match prompt_for_path("Choose your Music path.") {
        PathSelection::Selected(path) => path,
        PathSelection::Cancelled => return PathSelection::Cancelled,
        PathSelection::Invalid(NotWritable) => return PathSelection::Invalid(NotWritable),
        PathSelection::Invalid(PathError::Empty) => return PathSelection::Invalid(PathError::Empty),
        PathSelection::Invalid(NotDirectory) => return PathSelection::Invalid(NotDirectory)

    };
    PathSelection::Selected(music_path)
}

fn ask_video_path() -> PathSelection {
    let video_path = match prompt_for_path("Choose your Video path") {
        PathSelection::Selected(path) => path,
        PathSelection::Cancelled => return PathSelection::Cancelled,
        PathSelection::Invalid(NotWritable) => return PathSelection::Invalid(NotWritable),
        PathSelection::Invalid(PathError::Empty) => return PathSelection::Invalid(PathError::Empty),
        PathSelection::Invalid(NotDirectory) => return PathSelection::Invalid(NotDirectory)
    };
    PathSelection::Selected(video_path)
}

fn read_input() -> String {
    let mut user_try: i32 = 0;
    loop {
        let mut user_input = String::new();
        match io::stdin().read_line(&mut user_input) {
            Ok(_) => {
                return user_input.trim().to_string();
            }
            Err(err) => {
                user_try += 1;
                eprintln!("{}",format!("Couldn't read user input: {err}").bold().red());
                if user_try < 3 {
                    println!("{}", format!("Please retry ({user_try}/3)").bold().yellow());
                } else {
                    println!("{}","Too many attempts. Please re-open the program. If the problem persists, please report it.".bold().red());
                    thread::sleep(Duration::from_secs(3));
                    std::process::exit(1);
                }
            }
        }
    }
}

fn ask_confirmation(prompt: &str) -> bool {
    println!("{}", prompt.bold().cyan());
    loop {
        let user_input = read_input();
        match user_input.to_uppercase().as_str() {
            "Y" => return true,
            "N" => return false,
            _ => {
                println!("{}", "You must choose between Y and N.".bold().yellow());
            }
        }
    }
}