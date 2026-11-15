# jj-dlp

A desktop app written in Rust that watches streamers with [yt-dlp](https://github.com/yt-dlp/yt-dlp) and automatically records them when they go live. Runs on Windows and Linux.

## Features

### Monitoring

- One checker per site
    - All of a site's streamers are checked in a single yt-dlp call
- Check interval - How often to run the yt-dlp checker command for that site
- Checker command - The yt-dlp arguments or options to pass when checking for livestreams

### Recording

- Recording starts automatically when a stream goes live, and re-arms automatically afterwards
- Recorder command - The yt-dlp arguments or options to pass when downloading a livestream
- Output folder - The path to save the livestream file


### Streamers

- Add, pause and remove streamers
- Add a streamer by username - The stream URL is built from the site's URL template
- Streamer row
    - Settings button - opens the settings for that streamer
    - Open button - opens the streamer's output folder
- Pausing a streamer keeps checking if they are live, but never records

### Logging

- Optionally write the checker's and the recorder's stdout and stderr to log files, per site and per streamer

### Interface

- Themes - Switch between four themes
- Settings are saved automatically, and the settings folder can be opened from the GUI
- Window icon - Shown in the title bar and the taskbar

## Requirements

- [ffmpeg](https://www.ffmpeg.org/) - jj-dlp uses yt-dlp, which uses ffmpeg to download livestreams
    - If ffmpeg is missing, jj-dlp shows a setup notice with the command to install it (see [Setup notices](#setup-notices))
- [firefox](https://www.firefox.com/) - Firefox is needed so yt-dlp can use your cookie for each site
    - Open Firefox at least once so it creates its profile, and log in to the site if you want its cookies


## Monitor tab

- Streamers - Grouped by site
    - Site header - Shows `logging` when checker logging is on for the site, and whether the last check succeeded and how long ago it ran
        - A warning line is shown under the header when the last check hit an unexpected yt-dlp error (see [Problems](#problems))
    - Streamer row
        - Status - `OFFLINE` or `LIVE`, plus `RECORDING` while a recording is running, `PAUSED` when recording is off, `restarted N×` when the recorder has exited unexpectedly, and `logging` when recorder logging is on for that streamer
        - Warning line - Shown under the row when a known setup problem is stopping the recording (see [Problems](#problems))
        - Status dot - Glows green when live and recording is on, and glows grey when live but paused
        - Record toggle - Turn off to pause recording for that streamer
        - Settings button - Opens the settings for that streamer (see [Streamer settings](#streamer-settings))
        - Open button - Opens the streamer's output folder (the site's output folder, or the global one if the site has none)
        - Remove button - Deletes the streamer
    - Add streamer row - Shown at the bottom of each site (see [URL template](#url-template))
        - Display Name (optional) - The name shown for the streamer. Leave blank to use the username
        - Streamer username - The streamer's name on that site. A leading `@` is ignored
        - Add streamer button - Adds the streamer, using the site's URL template to build their stream URL
- Pausing or removing a streamer stops its running recorder
- A paused streamer is still checked, so its status stays current, but no new recorder starts
- Turning **Record** back on starts recording immediately if the stream is still live

## Setup notices

- Checked at launch and whenever settings change, and shown at the top of the Monitor tab as **SETUP NEEDED**
    - ffmpeg was not found - Shown when ffmpeg is not on your `PATH`. Not shown if a site's command uses `--ffmpeg-location`, or on Windows if `ffmpeg.exe` is in the yt-dlp folder
    - Firefox has no profile yet - Shown when a site's command uses `--cookies-from-browser firefox` and no Firefox cookie database is found
- Command - The command that installs ffmpeg on your system, with a **Copy** button
    - Windows - `winget install --id Gyan.FFmpeg --exact`
    - macOS - `brew install ffmpeg`
    - Linux - The package manager command for your distribution (apt, dnf, pacman, zypper, apk or xbps)
- **Re-check** button - Runs the checks again, and clears the recording problems shown under streamers
- Windows - Restart jj-dlp after installing ffmpeg, because a running app does not see new `PATH` entries
- Each notice is also written to the Log tab

## Problems

- Checker problems
    - An unexpected yt-dlp error is shown as a warning line under the site header, and the last check is shown as failed
    - A channel that is simply offline is not a problem
    - A check that fails without printing any error is also reported
    - The warning clears on the next check that succeeds
    - The Log tab gets one message when a problem appears or changes, and one when it clears
- Recording problems
    - When a recorder exits unexpectedly, jj-dlp writes one message to the Log tab and keeps retrying while the stream is live
    - Pausing, removing, quitting, and a recorder that exits normally are not problems
    - `restarted N×` - Counts the unexpected exits since the stream went live, and resets when the stream goes offline
    - A failure that repeats with the same error is logged only the first time
    - Known setup problems show a warning with the fix under the streamer's row
        - ffmpeg is not installed
        - Firefox has no profile yet
        - The browser has its cookie database locked
    - The warning clears when a recording has run for 30 seconds or ends normally, or when you press **Re-check**
- If yt-dlp finds no cookies in the browser, a message is written to the Log tab once per live session, because the recording then runs without a login

## Streamer settings

- Settings panel - Shown under the streamer's row when you click **Settings**. Changes apply immediately
    - Log recorder stdout - Write the recorder's stdout to this streamer's log file (see [Log files](#log-files))
    - Log recorder stderr - Write the recorder's stderr to this streamer's log file
- These are read when a recording starts, so changing them mid-recording applies to the next recording

## Processes tab

- Header - Shows how many are recording
- Recorder card - One per running recorder (see [Process panel](#process-panel-1))

## Settings tab

- Open settings folder button - Opens the config folder (see [Settings](#settings))

- Site card
    - Site name - The display name for the site
    - URL template - The stream address for the site, with `{username}` where the streamer's name goes (example: `https://twitch.tv/{username}`). Set this before adding streamers to the site
    - Check interval - How often to run the yt-dlp checker command for that site, in seconds (minimum 5)
    - Checker mode - Whether the checker uses `--dump-json` or `--simulate` (see below)
    - Path to yt-dlp executable - The full path to yt-dlp, just `yt-dlp` if it is on your `PATH`, or a relative path such as `bin\windows\yt-dlp\yt-dlp.exe` (see [yt-dlp path](#yt-dlp-path))
    - Checker command - The yt-dlp arguments or options to pass when checking for livestreams
    - Recorder command - The yt-dlp arguments or options to pass when downloading a livestream
    - Output folder - The path to save the livestream file. Leave blank to use the global output folder
    - Log checker stdout - Write the checker's stdout to the site's checker log file (see [Log files](#log-files))
    - Log checker stderr - Write the checker's stderr to the site's checker log file
    - Save changes button - Saves your edits
    - Revert button - Discards your edits
- Site templates -  Pre-created settings for some popular sites.
    - Clicking a template again does nothing if a site with that name already exists (names are compared ignoring case)
    - **Blank** can be added as many times as you like
- Edits are held as a draft until you click **Save changes**
- Removing a site also removes its streamers and stops their recorders
- Command arguments
    - Split on spaces. Use double or single quotes for values containing spaces
    - Backslashes are kept literally, so Windows paths work as written
    - The command is run directly, not through a shell
- Recorders run with the output folder as their working directory, so relative `-o` templates save there
- The output folder is created if it does not exist


## URL template

- Each site has a URL template, which is the stream address with `{username}` standing in for the streamer's name
- Adding a streamer saves the full URL built from the template, so changing a template later does not change streamers that were already added

## yt-dlp path

- New sites use the bundled yt-dlp for your platform
    - Windows - `bin\windows\yt-dlp\yt-dlp.exe`
    - Linux - `bin/linux/yt-dlp/yt-dlp`
    - Other systems - `yt-dlp`, found through your `PATH`
- A relative path is searched in this order, and the first file found is used
    - The folder containing jj-dlp
    - Up to three folders above it
    - The current working folder
- This works the same whether you start jj-dlp from its own folder or from another one
- `/` and `\` are both accepted in a relative path
- Absolute paths and plain names such as `yt-dlp` are used as written
- Linux - If the bundled yt-dlp is not executable, jj-dlp sets the executable bit and writes a message to the Log tab

## Process panel

- Recorder card
    - State - `STARTING`, `RECORDING`, `STOPPING` or `EXITED · CONFIRMING`
    - Uptime, the site, and the full command line, wrapped onto extra lines when it is long
    - bootloader - Shown when yt-dlp is a PyInstaller binary, which launches a second copy of itself
    - yt-dlp - The real yt-dlp process (the deepest process with the same name as the one launched)
    - ffmpeg - Each ffmpeg child, anywhere in the tree
    - Latest output line - The latest line of yt-dlp output, kept to a single line and cut off at the edge of the card
- A card is removed only after the launched process and every child seen in its tree have exited
- If stray children linger 20 seconds after the main process exits, they are killed

## Stopping behaviour

- When a recorder is stopped (pause, remove, or quitting the app), how it is stopped depends on your platform
													  
    - Linux - `SIGINT` to the recorder's process group so ffmpeg can finish the file, then `SIGKILL` if it is still running after 20 seconds
    - Windows - `taskkill /T /F` right away, which ends yt-dlp and ffmpeg together. ffmpeg may not finalise the file cleanly on a forced kill
- Closing the window stops all recorders before the app exits
- If the app itself is killed, running recorders keep going

## Command-line modes

- `jj-dlp` - Runs the app in GUI mode
- `jj-dlp --no-gui` - Runs the app without a window. Log messages are printed to the terminal
- `jj-dlp --engine-only` - Runs only the basic recording engine, without the settings layer or a window. Log messages are printed to the terminal
- Both headless modes read `settings.json` at launch and record every enabled streamer
- Press Ctrl+C to stop all recorders and exit (see [Stopping behaviour](#stopping-behaviour))
- `jj-dlp --help` lists the options

## Settings

- Settings are saved automatically as `settings.json` in the OS config folder
    - Linux - `~/.config/jj-dlp/`
    - Windows - `%APPDATA%\jj-dlp\config\`
- Open settings folder button - On the Settings tab, opens the config folder
- Files in the config folder
    - `settings.json` - Holds the global output folder, sites and streamers
    - `gui.json` - Holds the theme
    - `logs` folder - Holds the log files (see [Log files](#log-files))
- The default output folder is `jj-dlp` inside your Videos folder
- `settings.json` is read at launch. Edits made to it by hand while the app is running are not picked up until the next launch

## Log files

- Output logging is off by default
- Log files are written to the `logs` folder inside the settings folder
					 
- `twitch.checker.log` - One per site. Each check adds the timestamp, the command line, the exit code and duration, then the stdout and stderr you enabled
- `twitch.streamer_username.stdout.log` and `twitch.streamer_username.stderr.log` - One pair per streamer. Each line is timestamped (UTC). A recording adds a `start:` line (command line, working folder, process id) and an `exit:` line (exit code, runtime)
- Files are appended to. A file over 5 MB is emptied before the next write

## Log tab

- Messages - The most recent 500 engine messages (UTC): 
- streamers going live
- recordings starting
- failed checks
- check problems appearing and clearing
- recordings stopping unexpectedly
- setup notices
- no cookies found in the browser
- unmatched live entries
- yt-dlp files made executable
