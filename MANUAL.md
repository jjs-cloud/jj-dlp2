# jj-dlp

A desktop app written in Rust that watches streamers with [yt-dlp](https://github.com/yt-dlp/yt-dlp) and automatically records them when they go live. Runs on Windows, Linux and macOS.

## Features

### Monitoring

- One checker per site
    - All of a site's streamers are checked in a single yt-dlp call, except streamers that set their own check interval, which are checked separately from the batch (see [Streamer settings](#streamer-settings))
- Check interval - How often to run the yt-dlp checker command for that site, counted from when the previous check has finished. A streamer can set its own interval
- Checker command - The yt-dlp arguments or options to pass when checking for livestreams. Whether it uses `--dump-json` or `--simulate` is detected from the arguments (see [Settings tab](#settings-tab))
- Live time - Shows how long each live streamer has been live (see [Live time](#live-time))
- Last live - Shows how long ago each offline streamer was last live, and looks up the time for Twitch streamers at launch (see [Last live](#last-live))
- Consider a stream ended after (minutes) - How long a streamer must go unseen live before the live time resets, per site
- Pause checking - Stops a site's streamers from being checked, per site or per streamer, while recordings that are already running carry on (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
- Quick check - Checks a streamer again on its own shortly after its recording ends, so a stream that starts again is picked up without waiting for the site's next check, per site and per streamer (see [Quick check](#quick-check))

### Recording

- Recording starts automatically when a stream goes live, and re-arms automatically afterwards
- Recorder command - The yt-dlp arguments or options to pass when downloading a livestream
- Output folder - The path to save the livestream file, with **Browse** and **Open** buttons beside the field
- Recording subfolders - Saves each recording in a subfolder or nested subfolders of the output folder, per site and per streamer (see [Recording subfolders](#recording-subfolders))
- Intro delay - Waits a set number of minutes after a streamer goes live before recording starts, or records right away and splits the intro into its own file, per streamer (see [Intro delay](#intro-delay))
- Split recordings - Starts a new part of a recording after a set number of minutes, per site and per streamer (see [Split recordings](#split-recordings))
- Auto-suffix - Names a recording that starts again during the same stream with the next `_partN`, per site and per streamer (see [Auto-suffix](#auto-suffix))
- Upgrade quality - Restarts a recording once when the stream starts offering a higher quality than the one being recorded, per site and per streamer (see [Upgrade quality](#upgrade-quality))
- Priority - Limits how many streamers are recorded at once, and decides which streamers are recorded first when the limit is reached, with an **Always record** switch per streamer (see [Priority](#priority))
- Recording file checks - Shows a red `STALLED N×` chip when a recording's file is missing or stops growing, and keeps it until it is dismissed or clears itself, with the timings and the **Warn** switch set per site on the Notifications page (see [Recording file checks](#recording-file-checks))
- Restart stalled recordings - Stops a recording that has become `STALLED` and starts it again, up to a limit for each live time, per site (see [Restart stalled recordings](#restart-stalled-recordings))
- Restart warnings - Shows a `RESTARTED N×` chip when a recorder starts again after exiting unexpectedly, with the timings, the threshold and the **Warn** switch set per site on the Notifications page (see [Restart warnings](#restart-warnings))
- Output warnings - Shows `INTERRUPT N×` and `ERROR N×` chips when a recorder's output has lines that match the site's patterns, and hides `ERROR N×` when both have been triggered in the same live time, with the patterns and the **Warn** switches set per site on the Notifications page (see [Output warnings](#output-warnings))
- Resolution - Shows the resolution and frame rate of a recording once its file is confirmed, read with ffprobe (see [Resolution](#resolution))
- Open button on the Processes tab - Opens the file that is being recorded
- Clickable `RECORDING` chip - Glows and opens the file that is being recorded, once its file is confirmed, from the Livestreams tab and from Compact view (see [Livestreams tab](#livestreams-tab))
- Clickable warning chips - `STALLED N×`, `RESTARTED N×`, `INTERRUPT N×` and `ERROR N×` glow and dismiss their own warning for one streamer when clicked, from the Livestreams tab and from Compact view (see [Livestreams tab](#livestreams-tab))
- Clickable stream URL - Opens the stream URL of a streamer in your default browser, from the stream URL in the Livestreams tab and from the site tag in Compact view (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
- Command output panel - Shows the recorder's full output in the expanded recorder card on the Processes tab, with **Wrap** and **Copy** controls (see [Process panel](#process-panel-1))
- Watcher - Starts jj-dlp again automatically if it crashes or is killed, so recording carries on without anyone reopening it (see [Watcher](#watcher))
- Recorder cleanup - Stops recorder processes that a crash left running when jj-dlp starts again, so the same stream is not recorded twice (see [Recorder cleanup](#recorder-cleanup))


### Streamers

- Add, pause and remove streamers
- Add a streamer by username - Choose the site from the list in the add streamer row at the bottom of the Livestreams tab, in the normal view and in Compact view, and the stream URL is built from the site's URL template (see [Livestreams tab](#livestreams-tab))
- Sites are created as needed - Every site template is always in the site list, and its site is created the first time a streamer is added to it
- Add several streamers quickly - Pressing Enter in the add streamer row adds the streamer and keeps the cursor in the **Streamer username** box, ready for the next username
																																					   
- Streamer row
    - Settings button - opens the settings for that streamer, and is highlighted while they are open
														
- Pausing a streamer keeps checking if they are live, but never records
- Schedule - Switches a streamer on and off automatically at set times, one-off or repeating weekly, per streamer (see [Schedule](#schedule))
- Display name - Sets the name shown for a streamer from the streamer's settings, after the streamer has been added (see [Streamer settings](#streamer-settings))
- Remove button in Compact view - Removes a streamer right away from its row, with an `x` beside the Record toggle (see [Compact view](#compact-view))

### Files

- Files tab - Lists the recordings in every site's output folder, with a live status, size and write rate for each file (see [Files tab](#files-tab))
- Sort and group - Click a column header to sort, and group the files by folder, streamer, site, or a site and streamer together, with groups that collapse and expand (see [Files tab](#files-tab))
- File streamer and site - Remembers which streamer and site each recording came from, so files can be grouped by streamer or site even when they are not saved in subfolders (see [Files tab](#files-tab))
- Open, Show in folder and Delete - Opens a recording, shows it in your file manager, or deletes it to the recycle bin or permanently
- Fixup, Trim and Move - Repairs a recording, cuts a section out of it, or moves it to any folder you choose
- Cancel - Stops a running Fixup, Trim or Move and removes the partly written file
- Hide posts - Hides the files of downloaded posts from the list, so they do not show up when a posts folder is also a recording folder (see [Files tab](#files-tab))

### Logging

- Optionally write the checker's and the recorder's stdout and stderr to log files, per site and per streamer
    - Recorder logging can be turned on or off during a recording, from the Output sub-tab (see [Output sub-tab](#output-sub-tab))
    - Open logs folder button - Opens the folder that holds the log files, with the file that is open in the viewer selected, from the Output sub-tab (see [Output sub-tab](#output-sub-tab))
- Posts logging - Optionally write the stdout and stderr of each Posts account's checker and of each Posts site's downloader to log files, turned on from the Output sub-tab (see [Output sub-tab](#output-sub-tab))
- Failed commands - A checker or downloader command that times out or cannot be started is also written to its log files, with the reason (see [Log files](#log-files))
- Output sub-tab - Lists the log files of every site and streamer, and of the checkers and downloaders of the Posts sites, with a switch to turn each one on or off and a viewer that shows the newest lines of a file and follows it as it grows (see [Output sub-tab](#output-sub-tab))
- Log tab history - Messages are also saved to `ui.log.txt` and shown again after jj-dlp restarts, with times in your local time (see [Log tab](#log-tab))
- Open Log button - Opens `ui.log.txt` in your default app, from the top right of the Log tab (see [Log tab](#log-tab))
- Open Changelog button - Opens the What's new window, from the top right of the Log tab, beside the Open Log button (see [Changelog](#changelog))
- Crash log - A panic on any thread is written to `crash.log` with its location and a backtrace, and the next launch reports a previous session that did not exit cleanly. The watcher also writes how jj-dlp ended each time it was started again (see [Crash log](#crash-log) and [Watcher](#watcher))
- Restart entry - The Log tab shows when jj-dlp was started again by the watcher, and when recorders left running by a crash were stopped (see [Log tab](#log-tab))
- Performance log - `--perf` writes slow operations, message counts and window stalls to `perf.log` (see [Command-line modes](#command-line-modes))

### Notifications

- Notifies you when a streamer goes live, and optionally when a recording becomes `STALLED` or `RESTARTED N×`, or when a recorder's output matches an interrupt or error pattern (see [Notifications](#notifications))
- Desktop notifications - A native toast on Windows, and `notify-send` on Linux
- ntfy notifications - Sends the notification to your phone with [ntfy](https://ntfy.sh/)
- Notification settings - Set per site on the Notifications page of the Settings tab, with a per-streamer override that follows the site or turns notifications on or off (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
- Notify and Warn list - Turns the notification of each kind (**Live**, **Stalled**, **Restarts**, **Interrupts** and **Errors**), the warning chip of each kind except **Live**, and its flashing, on or off separately, per site, and an **Enable Warnings** switch hides all of a site's warning chips at once (see [Settings tab](#settings-tab))
- Global switches - Turns off every notification, or every warning, for all sites at once without changing the sites' own settings, and sets an ntfy topic that sites with no topic of their own use, which can be filled with a random one (see [Notifications](#notifications))
- Send test button - Sends a test notification from the Notifications page, so the settings can be checked before saving
- Free space line - Ends every notification with the free space left on the drive of the output folder it is about, and warns when the space is low (see [Disk space](#disk-space))

### Posts

- Posts tab - Checks the accounts you add for new posts and downloads them, shown with thumbnails (see [Posts](#posts))
- Library lists - **Unviewed**, **Viewed**, **Starred** and **Failed** lists in the sidebar, with a search box, a sort order and a card size (see [Posts](#posts))
- Background checking - Checks and downloads carry on on every tab, and also with `--no-gui` (see [Posts](#posts) and [Command-line modes](#command-line-modes))
- Add and remove accounts - Add an account from the bar at the bottom of the Posts tab, and remove the selected account, along with its posts, with the button at the bottom of the sidebar
- Account ID lookup - Adds an account from the URL of one of its videos, and keeps the account's ID for checks that cannot find the account from its profile address alone (see [Account ID lookup](#account-id-lookup))
- Site settings - Every Posts setting is set per site on the Settings tab, with **Save changes**, **Revert** and **Defaults** buttons (see [Posts site settings](#posts-site-settings))
- Cancel - Stops a post that is queued or downloading, and goes back to **Save** (see [Posts](#posts))
- Auto-delete - Deletes the files of unstarred posts after a set number of days, per site (see [Auto-delete](#auto-delete))
- Open m3u8 instead of site - Plays a post that is not downloaded from its stream link, instead of opening its page, per site (see [Open m3u8 instead of site](#open-m3u8-instead-of-site))
- Notifications - Desktop and ntfy notifications about new posts, set per site on the Notifications page (see [Posts](#posts))
- Auto-download limits - Skips an automatic download that is larger than a set size or takes longer than a set time, per site (see [Auto-download limits](#auto-download-limits))
- Twitter/X Helper - Finds the newest posts of an X account with the basic HTML scraper or with gallery-dl, chosen per site with a dropdown, and downloads the pictures of photo posts with gallery-dl (see [Twitter/X helper](#twitterx-helper))
- TikTok Photos scraper - Saves the pictures of a TikTok photo post, with one card for each picture, turned on per site with a switch (see [TikTok photos scraper](#tiktok-photos-scraper))

### Interface

- Themes - Switch between four themes
- Site color - Choose a color for each site, used for its name and tags (see [Site color](#site-color))
- Disk space - Shows the free space of every drive that holds an output folder in the header, to the left of the theme list, and turns it to a warning color when the space is low. Extra drives can be added, the drives can be put in any order, and the percentage free, the word `free` and the low limits can be set on the General page of the Settings tab (see [Disk space](#disk-space))
- Status chips - Shows how many streamers are being monitored, how many are live and how many are recording, in the header (see [Livestreams tab](#livestreams-tab))
- Top tabs - **Livestreams** and **Posts** are separate tabs in the tab bar, with **Posts** to the right of **Livestreams** (see [Livestreams tab](#livestreams-tab) and [Posts](#posts))
- Compact view - Shows each streamer as a single row that shrinks as more streamers are added, turned on with the **Compact View** button (see [Compact view](#compact-view))
- Full view columns - Splits the streamers of each site panel into two columns when the window is wide enough, with the minimum column width set on the General page of the Settings tab (see [Livestreams tab](#livestreams-tab) and [Settings tab](#settings-tab))
- Live first - Moves the streamers that are live to the top, in a **Live** panel in the normal view and at the start of the list in Compact view, turned on and off from the first entry of the sort list (see [Sorting](#sorting))
- Sort order - Sorts the streamers by name, when they were added, when they were last live or their priority, in ascending or descending order, from a sort list that always shows the current sort order (see [Sorting](#sorting))
- Group by site - Turns the grouping of the streamers by site on or off, in the normal view and in Compact view, from the second entry of the sort list (see [Sorting](#sorting))
- Site order - Moves a site up or down with the **▲** and **▼** buttons on its panel on the Livestreams tab, which sets the order of the sites in the normal view and in Compact view while **Group by site** is on (see [Livestreams tab](#livestreams-tab))
- Warning pulse - Pulses the **Livestreams** tab button with an amber-to-red gradient while a warning chip is showing, on every tab, unless **Flash** is turned off for the kind of the chip that is showing (see [Livestreams tab](#livestreams-tab))
- Idle animation - The pulsing in the title and on the status dots only runs while something is live, recording or needs attention, so jj-dlp uses very little CPU while it is idle. It also pauses while the window is minimized
- Row colors - Turns a streamer's row red while a recording is stalled and yellow while `STALLED N×` is still shown after the recording has recovered, or while `RESTARTED N×`, `INTERRUPT N×` or `ERROR N×` is shown, in both the streamer row and Compact view, unless **Flash** is turned off for the kind of the chip (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
- Flash switches - Turn the pulsing of the **Livestreams** tab button and the coloring of a streamer's row off for one kind of warning (**Stalled**, **Restarts**, **Interrupts** or **Errors**) of a site, while its warning chips and the **Clear warnings** button still work as normal, set per site on the Notifications page (see [Settings tab](#settings-tab))
- Settings sub-tabs - Switches the Settings tab between **Livestreams**, **Posts**, **Priority**, **Notifications** and **General** (see [Settings tab](#settings-tab))
- Site templates - Every site template is always available in the site list when adding a streamer or a Posts account, so no site needs to be set up first (see [Livestreams tab](#livestreams-tab), [Posts](#posts) and [Settings tab](#settings-tab))
- Settings are saved automatically, and the settings folder can be opened from the GUI
- Config problem banner - Warns when a settings file cannot be loaded, keeps a `.bad` copy of the broken files and does not save until you choose **Start fresh** (see [Config problems](#config-problems))
- Site defaults - Most Site card fields can be left blank to use the default shown in the field (see [Site defaults](#site-defaults))
- Window icon - Shown in the title bar and the taskbar
    - Linux - jj-dlp adds a launcher entry and an icon file under `~/.local/share` at launch, so desktops that take the icon from a launcher entry show it (see [Settings](#settings))
- Centered window - Opens in the center of the screen, and opens smaller when the window would not fit the screen, such as with a large display scale setting (see [Settings tab](#settings-tab))
- Version number - Shown beside the jj-dlp title at the top of the window
- UI scale - Scales the whole window so jj-dlp looks the same on small laptop screens and large monitors, chosen automatically from the monitor or set by hand on the General page of the Settings tab (see [Settings tab](#settings-tab))
- Remember window size - Optionally reopens the window at the size it had when it was last closed (see [Settings tab](#settings-tab))
- Single instance - Only one jj-dlp can run at a time (see [Single instance](#single-instance))
- Settings folder option - `--config-dir=<path>` keeps the settings in another folder, so more than one jj-dlp can run at the same time (see [Command-line modes](#command-line-modes))
- Graphics option - `--backend=<name>` chooses how the window is drawn, for a window that freezes or stutters (see [Command-line modes](#command-line-modes))
- Watcher option - `--no-watcher` runs jj-dlp without the watcher that starts it again after a crash (see [Command-line modes](#command-line-modes))

### Updates

- Checks GitHub for a new release at launch and every 6 hours, and shows **Update available!** in the GUI (see [Updates](#updates))
- One-click install - Downloads the release, checks it, stops the recorders and restarts jj-dlp in the new version
- Release candidates - Optional, off by default
- `jj-dlp --update` - Installs the latest release from the command line
- Settings resets - A release can set chosen settings back, once, the first time the new version starts (see [Settings resets](#settings-resets))
- What's new - The changelog opens after each update and can be reopened from the Updates card (see [Changelog](#changelog))

## Requirements

- [ffmpeg](https://www.ffmpeg.org/) - jj-dlp uses yt-dlp, which uses ffmpeg to download livestreams
    - If ffmpeg is missing, jj-dlp shows a setup notice with the command to install it (see [Setup notices](#setup-notices))
    - The Files tab runs ffmpeg for Fixup, Trim and Move, and only looks for it on your `PATH` (see [Files tab](#files-tab))
    - The Posts tab runs ffmpeg from your `PATH` to make a thumbnail for a downloaded post that has no preview image (see [Posts](#posts))
- ffprobe - Comes with ffmpeg, and is used to read the resolution and frame rate of a recording (see [Resolution](#resolution))
    - If ffprobe is not on your `PATH`, no resolution is shown and a message is written to the Log tab
    - Upgrade quality needs ffprobe to read the recording (see [Upgrade quality](#upgrade-quality))
- gallery-dl - Needed for the **gallery-dl** Twitter/X helper. A copy is bundled for Windows and Linux (see [gallery-dl path](#gallery-dl-path))
- [firefox](https://www.firefox.com/) - Firefox is needed so yt-dlp and gallery-dl can use your cookie for each site
    - Open Firefox at least once so it creates its profile, and log in to each site if you want it's cookies


## Livestreams tab

																																																			 
															   
- Posts tab - Shown to the right of the **Livestreams** tab in the tab bar, and holds the posts of the accounts you have added (see [Posts](#posts))
    - While there are unviewed posts, the tab shows how many, in brackets (example: `Posts (3)`)
- Status chips - Shown in the header, as `N monitoring`, `N live` and `N recording`
    - `N monitoring` - The number of streamers that have **Record** turned on and are on a site that is not paused (see [Settings tab](#settings-tab))
    - `N live` - The number of streamers that are live
    - `N recording` - The number of recorders that are running
- Title row - Shown at the top of the tab, with the **Clear warnings** button, when it is shown, and the **Compact View** button, the sort list and the sort direction button on the right (see [Compact view](#compact-view) and [Sorting](#sorting))
- Switching to the Livestreams tab closes every open streamer settings panel, and scrolls the list back to the top (see [Streamer settings](#streamer-settings))
- Checker problems - Shown as warning lines at the top of the list, above the **Live** panel and the site panels, in the normal view (see [Problems](#problems))
    - One line is shown for each site whose last check hit an unexpected yt-dlp error, as `<site>: <message>`, or `<site>: last check failed` when the check failed without a message
    - The lines are the same as the ones shown above the list in Compact view (see [Compact view](#compact-view))
- Live panel - Shown above every site panel while **Live first** is on and at least one streamer is live. It holds the live streamers of every site, and is not shown while none are live (see [Sorting](#sorting))
    - The rows are the same as the streamer rows in the site panels
    - The streamers are in the chosen sort order, and are not grouped by site
    - A streamer that is live is not shown in its site panel while the panel is shown
- Streamers - Grouped by site while **Group by site** is on, and not grouped by site while it is off (see [Sorting](#sorting))
    - Site panel - Only shown while **Group by site** is on and the site has at least one streamer. While **Live first** is on, only the streamers that are not live are counted, so a site whose streamers are all live has no panel until one of them goes offline
    - Columns - When the window is at least twice the **Column Width Min** of the **FULL VIEW** section wide, the streamer rows of a site panel, and of the **Live** panel, are split into two columns, and into one column when it is narrower (see [Settings tab](#settings-tab))
        - The site header stays across the full width of the panel, above the columns
        - The rows fill the left column first and then the right, so 5 streamers are shown as 3 and 2
        - A panel with one streamer stays in one column
        - The site panels are never placed side by side
    - Site header - Shows the site name in its site color (see [Site color](#site-color)), `LOGGING` when checker logging is on for the site, `PAUSED` when checking is paused for the site (see [Settings tab](#settings-tab)), and whether the last check succeeded and how long ago it ran
																																 
																	 
        - The site's `PAUSED` chip is separate from the `PAUSED` status of a streamer. While it is shown, no new checks run for the site
        - Order buttons - **▲** and **▼**, shown at the top right of the panel, to the right of the check time. Each moves the site up or down one place, swapping it with the neighbouring site panel that is shown. **▲** is unavailable on the first panel and **▼** on the last
            - The order is saved, and it also sets the order of the sites in Compact view (see [Compact view](#compact-view) and [Sorting](#sorting))
    - Streamer row
        - Status - `OFFLINE` or `LIVE`, plus `RECORDING` while a recording is running (hidden once the streamer is paused), `STALLED N×` when the recording file is missing or has stopped growing, where N is the number of stalls in the live time (see [Recording file checks](#recording-file-checks)), `PAUSED` when recording is off, `SCHEDULED` when the streamer has a schedule turned on (see [Schedule](#schedule)), `RESTARTED N×` when the recorder has started again after exiting unexpectedly and the count has reached the site's threshold (see [Restart warnings](#restart-warnings)), `INTERRUPT N×` and `ERROR N×` when the recorder's output has lines that match the site's interrupt or error patterns (see [Output warnings](#output-warnings)), and `LOGGING` when recorder logging is on for that streamer
            - `RECORDING` chip - Becomes a button that glows once the recording file is confirmed, and opens the file in your default app, in the same way as the **Open** button on the Processes tab (see [Recording file checks](#recording-file-checks) and [Processes tab](#processes-tab)). Until then it is shown as a plain chip
            - Warning chips - `STALLED N×`, `RESTARTED N×`, `INTERRUPT N×` and `ERROR N×` are each shown only while the **Warn** switch of their kind is on for the site and **Enable Warnings** is on for the site, and none are shown while all warnings are disabled globally (see [Settings tab](#settings-tab))
            - `ERROR N×` is hidden while `INTERRUPT N×` has also been triggered for the streamer in the same live time, and stays hidden after the `INTERRUPT N×` chip is dismissed or clears itself. It is shown again from the first error of a new live time (see [Output warnings](#output-warnings))
            - Dismissing a warning - Each warning chip is a button that glows, in the same way as the `RECORDING` chip. Clicking it dismisses that warning for that streamer only, and no other streamer is changed
                - `RESTARTED N×` - Hides the chip and keeps the streamer's restart count (see [Restart warnings](#restart-warnings))
                - `INTERRUPT N×` and `ERROR N×` - Hide the chip of that kind only, and the other kind is left as it is. The count is kept, so a later matching line carries it on, or starts it again from 1 in a new live time (see [Output warnings](#output-warnings))
                - `STALLED N×` - Hides the chip and keeps the streamer's stall count, in the same way as the **Clear warnings** button (see [Recording file checks](#recording-file-checks))
                - The row color goes back to normal once no warning chip is left showing on the row
        - Row color - The whole row turns red while a recording is stalled, and yellow while `STALLED N×` is shown after the recording has recovered, or while `RESTARTED N×`, `INTERRUPT N×` or `ERROR N×` is shown. Red is used when a recording is stalled together with a yellow chip, and the row returns to its normal color when the chips disappear. A chip colors the row only while **Flash** is turned on for its kind on the site (see [Settings tab](#settings-tab))
        - Warning line - Shown under the row when a known setup problem is stopping the recording (see [Problems](#problems))
        - Resolution - The width and height of the recording, followed by its frame rate when it is known, in grey, shown right before the live time (example: `1920 x 1080 · 60 fps`). Only shown once the recording file is confirmed (see [Resolution](#resolution))
        - Live time - `live for` and how long the streamer has been live, shown right after the streamer name, on the same line. Only shown while the streamer is live (see [Live time](#live-time))
        - Stream URL - Shown under the streamer name, with its domain in the site's color (see [Site color](#site-color))
            - It is a link. Clicking it opens the stream URL in your default browser, and the whole URL becomes brighter while the pointer is over it
            - Only URLs that start with `http://` or `https://` are opened
        - Last live - `last live` and how long ago the streamer was last live, shown after the stream URL under the streamer name (example: `last live 3h ago`). Hidden while the streamer is live, and when no time is known yet (see [Last live](#last-live))
        - Status dot - Glows green when live and recording is on, and glows grey when live but paused
        - Record toggle - Turn off to pause recording for that streamer
        - Settings button - Opens the settings for that streamer, and is highlighted while they are open (see [Streamer settings](#streamer-settings))
																																																																							 
        - Remove button - Deletes the streamer
- Offline panel - Shown in place of the site panels while **Group by site** is off (see [Sorting](#sorting))
    - It holds every streamer that is not live, from every site, in the chosen sort order, in the same rows as the site panels
    - It is titled **Offline** while **Live first** is on, and **Streamers** while **Live first** is off, when every streamer is listed in it, live or not
    - It is only shown while it has at least one streamer
    - There are no site headers, so the site's `LOGGING` and `PAUSED` chips, its check time and the **▲** and **▼** buttons are not shown. The site is still shown in each row, in the domain of the stream URL
    - Checker problems are still shown at the top of the list
- Add streamer row - Shown at the bottom of the tab, in the normal view and in Compact view (see [URL template](#url-template) and [Compact view](#compact-view))
    - Site list - Chooses the site the streamer is added to. It lists every site template and every site you have set up on the Settings tab. A template that has no site yet gets its site, with the template's settings, when the first streamer is added to it (see [Settings tab](#settings-tab))
        - The chosen site stays chosen after a streamer is added
																								
    - Streamer username - The streamer's name on the chosen site. A leading `@` is ignored, and the username is added in lowercase letters. After you add a streamer, the cursor stays in this box, so you can type the next username and press Enter again. The streamer is shown under its username until a display name is set in its settings (see [Streamer settings](#streamer-settings))
    - Add streamer button - Adds the streamer, using the site's URL template to build their stream URL
    - First check - A new streamer is checked once on its own. The site's other streamers are not checked again because of it (see [Quick check](#quick-check))
    - Enter - Pressing Enter in **Streamer username** has the same effect as the **Add streamer** button. Nothing is added when the username is blank, and the cursor stays in **Streamer username**
    - Pulse - While no streamer has been added yet, the **Streamer username** box pulses so it is easier to find. It stops pulsing once a streamer has been added
- Empty page - `No streamers yet. Pick a site and add a username below.` is shown while no streamer has been added, in the normal view and in Compact view
- Clear warnings button - Shown in the title row, only while at least one `RESTARTED N×`, `INTERRUPT N×`, `ERROR N×` or `STALLED N×` chip is showing. Hides every chip, and keeps the restart, interrupt, error and stall counts of every streamer, which start again from 1 only when the next warning of that kind comes in a new live time (see [Restart warnings](#restart-warnings), [Output warnings](#output-warnings) and [Recording file checks](#recording-file-checks))
- Pulsing tab button - While at least one of those chips is showing for a streamer whose site has **Flash** turned on for the kind of that chip, the **Livestreams** tab button pulses with an amber-to-red gradient in place of its usual one. It pulses on every tab, and stops when those chips are gone
    - While **Flash** is turned off for a kind on a site, the chips of that kind do not make the tab button pulse (see [Settings tab](#settings-tab))
- Pausing or removing a streamer stops its running recorder
- A paused streamer is still checked, so its status stays current, but no new recorder starts
- Pausing a streamer that is live keeps it shown as live, with `LIVE` and the live time, until a check no longer sees it live
- Turning **Record** back on starts recording immediately if the stream is still live, unless the streamer's intro delay has not ended yet (see [Intro delay](#intro-delay))

## Compact view

- Compact View button - Shown in the title row of the Livestreams tab, to the left of the sort list. Switches to Compact view, which replaces the site blocks with one single-line row per streamer. In Compact view the button reads **Full View**, and pressing it goes back to the normal view
    - Order - While **Group by site** is on, the streamers are grouped by site, in the chosen sort order. While **Live first** is on, the streamers that are live are listed first, in the chosen sort order and not grouped by site, followed by the rest grouped by site. While **Group by site** is off, the streamers are not grouped by site, and are listed in the chosen sort order, with the streamers that are live first while **Live first** is on (see [Sorting](#sorting))
    - Site order - While **Group by site** is on, the sites follow the order set with the **▲** and **▼** buttons of the site panels (see [Livestreams tab](#livestreams-tab))
    - Row - Status dot, site, streamer name, resolution, time, status chips, the Record toggle and the Remove button
    - Status chips - The same chips as the streamer row (see [Livestreams tab](#livestreams-tab)), except that `RECORDING` is shown as `REC`, `LOGGING` is shown as `LOG` and `SCHEDULED` is shown as `SCH`
        - `PAUSED` is shown before `LIVE`, and `REC` is hidden as soon as the streamer is paused, so a paused streamer that is still live shows `PAUSED` and `LIVE`
        - `LIVE` is hidden while `REC` is shown, because a recording already means the streamer is live
        - `REC` - Becomes a button that glows once the recording file is confirmed, and opens the file in the same way as the `RECORDING` chip
        - `STALLED N×`, `RESTARTED N×`, `INTERRUPT N×` and `ERROR N×` - Glowing buttons in the same way as on the streamer row. Clicking one dismisses that warning for that streamer only (see [Livestreams tab](#livestreams-tab))
    - Row color - The whole row is colored red or yellow in the same way as the streamer row (see [Livestreams tab](#livestreams-tab)). The color replaces the alternating row shading while it is shown, and a chip's color is not shown while **Flash** is turned off for its kind on the site
    - Site column - Sized to fit the longest site name, up to 12 characters, so the name column starts right after it. Longer names are cut off. The site is shown as a tag in its site color (see [Site color](#site-color))
        - The tag is a button. Clicking it opens the stream URL of the streamer in your default browser, and it glows while the pointer is over it
    - Name column - Takes the width left over by the other columns, so the time, the status chips and the Record toggle are never pushed out of the row. Names longer than the **Name length** are shortened to that length and end with `…`, and hovering a shortened name shows the full name
        - When a streamer has many chips, such as `REC`, `STALLED 2×`, `RESTARTED 2×` and `LOG`, the name is cut off to make room for them, without `…`
    - Time column - While the streamer is live, how long they have been live, without the `live for` label and in the live color, in a monospaced font, with seconds (example: `1h 05m 03s`). Otherwise how long ago they were last live, without the `last live` label (example: `3h ago`). The column is empty when the streamer is offline and no time is known (see [Live time](#live-time) and [Last live](#last-live))
        - The column is shown right before the status chips, so it moves left or right as chips appear or disappear
        - It has a fixed width, so the times line up from row to row
    - Resolution - The smaller of the recording's width and height, followed by `p` and then its frame rate when it is known, in grey (example: `1080p60`, and also `720p` for a vertical recording that is 720 x 1280 when its frame rate is not known). It is shown to the left of the time, right against it, and only once the recording file is confirmed (see [Resolution](#resolution))
    - Remove button - An `x` with no border, shown right beside the Record toggle at the end of the row. Pressing it removes the streamer at once, without asking, in the same way as the **Remove** button of the streamer row (see [Livestreams tab](#livestreams-tab))
        - It is muted until the pointer is over it, and then changes color
    - Site headers and the Settings button are hidden. Press **Full View** to use it
    - Add streamer row - Shown under the list, in the same way as in the normal view (see [Livestreams tab](#livestreams-tab))
    - Checker problems are shown as warning lines above the list
- Row size - Rows start at the maximum size and shrink toward the minimum size as streamers are added, so that every streamer fits without scrolling
    - The text, chips, status dot, Record toggle and Remove button shrink along with the row
    - The row size follows the window height, so resizing the window changes it
    - The row size leaves room for the add streamer row
- Columns - Three columns when the window is at least three times the **Column Width Min** wide, two columns when it is at least twice that wide, and one column when it is narrower
    - The number of streamers does not change the column count
    - With two columns, the streamers fill the left column first and then the right, so 5 streamers are shown as 3 and 2. With three columns they fill the columns in the same way, left to right
    - A single streamer fills the full width
    - Each column is outlined with a thin border. With more than one column, all borders are the same height, even when the last column has fewer streamers
    - Rows shrink toward the minimum size to fit in either layout, and the list scrolls when the rows do not fit
- Row sizes, the column width and the name length are set in the **COMPACT VIEW** section of the Compact view card on the Settings tab (see [Settings tab](#settings-tab))

## Sorting

- Sorting - Sets the order of the streamers on the Livestreams tab, so the streamers you care about are found first
    - Set with the sort list and the sort direction button in the title row of the Livestreams tab, with the **Compact View** button to the left of the sort list (see [Livestreams tab](#livestreams-tab))
    - Applies to the normal view and to Compact view
    - The settings are saved (see [Settings](#settings))
- Sort list - Chooses what the streamers are sorted by, and whether **Live first** and **Group by site** are on
    - The list always shows the current sort order when it is closed, also while **Live first** is on
    - **Live first** - The first entry of the list, with a check mark to the right of the text while it is on. Choosing it turns it on or off and closes the list, and the sort order is not changed
    - **Group by site** - The second entry of the list, with a check mark to the right of the text while it is on. Choosing it turns it on or off and closes the list, and the sort order is not changed
    - A horizontal line - Shown under **Group by site**, and separates the two switches from the sort options. Choosing the line closes the list without changing anything
    - **Name** - By the name shown for the streamer, ignoring upper and lower case
    - **Added** - By when the streamer was added. Ascending puts the streamer that was added first at the top
    - **Last live** - By when the checker last saw the streamer live (see [Last live](#last-live)). Ascending puts the streamer that was seen live longest ago at the top, and descending puts the most recent first. A streamer with no time yet counts as older than every other streamer
        - A streamer that is live counts as the most recent, and live streamers are equal to each other, so they stay in Priority order and do not change places as the checker sees them live again
    - **Priority** - By the order of the Priority list. Ascending puts the highest priority at the top (see [Priority](#priority))
    - Streamers that are equal for the chosen sort are kept in Priority order
- Direction button - Shown to the right of the sort list. Click it to switch between ascending and descending
    - Ascending - An up arrow followed by horizontal lines that are longest at the top
    - Descending - A down arrow followed by horizontal lines that are longest at the bottom
- Grouping - While **Group by site** is on, the streamers stay grouped by site, in the order of the sites set with the **▲** and **▼** buttons of the site panels, and the sort order applies inside each site. The only time they are not grouped by site is while **Live first** is on, for the streamers that are live
- Group by site - Groups the streamers by their site, or lists them together
    - Turned on and off with the second entry of the sort list. The choice is saved (see [Settings](#settings))
    - On - The normal view shows a site panel for each site, and Compact view groups the streamers by site
    - Off - The streamers are not grouped by site, and the sort order applies to all of them together
        - Normal view - Two panels are shown while **Live first** is on: the **Live** panel, with the streamers that are live, and an **Offline** panel, with every other streamer from every site. While **Live first** is off, one **Streamers** panel holds every streamer (see [Livestreams tab](#livestreams-tab))
        - Compact view - The streamers are listed in one list, with the streamers that are live first while **Live first** is on (see [Compact view](#compact-view))
- Live first - Moves the streamers that are live to the top of the list
    - Turned on and off with the first entry of the sort list
    - Normal view - The live streamers are taken out of their site panels and shown in the **Live** panel, which is always the first panel. The panel is not shown while no streamer is live (see [Livestreams tab](#livestreams-tab))
    - Compact view - The live streamers are listed first, before the rest (see [Compact view](#compact-view))
    - The live streamers are sorted by the chosen sort order, so **Live first** works with every option in the sort list
    - A streamer counts as live while it is shown as `LIVE`, including a streamer that is paused (see [Livestreams tab](#livestreams-tab))
    - A streamer moves back to its site when it is no longer live

## Live time

- Live time - How long a streamer has been live, counted from the first time the checker saw them live
    - Shown for each streamer on the Livestreams tab, in both the streamer row and Compact view (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - Only shown while the checker reports the streamer live, so it is hidden when the streamer is offline, where [Last live](#last-live) is shown instead
    - The streamer row shows seconds (example: `live for 1h 05m 03s`), and Compact view shows the same time without the label (example: `1h 05m 03s`)
    - The time is shown in a monospaced font, so the digits keep their width as the time changes. The `live for` label stays in the normal font
    - If a recorder exits unexpectedly, the live time is hidden until the next check sees the streamer live again, and then continues from the original start
    - Pausing a streamer does not hide the live time, because the recorder is stopped on purpose
- The live time starts over only when the checker has not seen the streamer live for the number of minutes set in **Consider a stream ended after (minutes)** on the site
    - A streamer who goes offline and comes back sooner than that keeps the same live time
    - A check that failed is not counted as the streamer being offline
        - When the yt-dlp error names a streamer, only that streamer's live time is kept. The other streamers of the site still end normally when the checker no longer sees them live (see [Problems](#problems))
        - When a check fails without naming a streamer, for example when it times out or yt-dlp cannot start, the live times of every streamer of the site are kept
    - The time is measured from the last check that saw the streamer live
- Paused streamers are still checked, so their live time is tracked the same way
- The live time is saved, and is shown again after jj-dlp restarts
    - When jj-dlp starts, a live time is dropped if the streamer was last seen live longer ago than the site's setting, so a stream that ended while jj-dlp was closed is not counted
    - If jj-dlp was closed for less than that time, the live time continues from the original start. If it was closed for longer, a stream that is still live starts counting again from the first check after launch
- Removing a streamer removes its live times

## Last live

- Last live - How long ago a streamer was last live, counted from the last check that saw them live
    - Shown for each streamer on the Livestreams tab, in both the streamer row and Compact view (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - Only shown while the streamer is not live, so it is never shown together with the live time
    - Hidden when no time is known yet. A streamer is not shown as `never` or `unknown`
    - The streamer row shows `last live` and the time (example: `last live 3h ago`), and Compact view shows only the time (example: `3h ago`), in the same column that shows the live time while the streamer is live
    - The time is shown in its largest whole unit: `<1m ago`, minutes, hours or days (examples: `5m ago`, `3h ago`, `2d ago`)
- Every check that sees the streamer live updates the time, so it is the last time the checker saw them live, not the time the recording ended
    - A check that failed does not change it
    - Paused streamers are still checked, so their time is tracked the same way
- Unlike the live time, it is not cleared by **Consider a stream ended after (minutes)**
- The time is saved, and is shown again after jj-dlp restarts
- Removing a streamer removes its last live times
- Twitch lookup - At launch, jj-dlp asks Twitch when each Twitch streamer last broadcast, so streamers who are offline have a time before they are next seen live
    - Only streamers whose stream URL is on `twitch.tv` and who have no last live time yet are looked up. Streamers on other sites get their time the first time the checker sees them live
    - The lookup runs at most once every 24 hours. A lookup that fails still counts, so it is not tried again until 24 hours later
    - Nothing is sent, and nothing counts, when every Twitch streamer already has a time
    - A streamer added after launch is not looked up until the next launch that is at least 24 hours after the last lookup
    - Twitch gives the time the last broadcast started, not when it ended, so a looked-up time can be older than the real end
    - A time the checker has already seen is never replaced by the lookup, and a looked-up time is replaced the next time the checker sees the streamer live
    - The Log tab gets one message when times are found, and one when the request fails (see [Log tab](#log-tab))

## Resolution

- Resolution - The size and frame rate of the video in a recording, read with ffprobe, so you can see at a glance what is being recorded
    - Shown for each streamer on the Livestreams tab, in both the streamer row and Compact view (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - The streamer row shows the width and the height, followed by the frame rate, in grey (example: `1920 x 1080 · 60 fps`)
    - Compact view shows the smaller of the two, followed by `p` and the frame rate, in grey (example: `1080p60`). A vertical video that is 720 x 1280 at 30 frames per second is shown as `720p30`
    - The frame rate is rounded to a whole number. It is left out when ffprobe cannot report one, and the resolution is still shown on its own
- When it is read - Once the recording file is confirmed (see [Recording file checks](#recording-file-checks))
    - A recording that is never confirmed shows no resolution
    - Each new part of a recording is read again when its file is confirmed (see [Split recordings](#split-recordings))
    - The resolution is hidden once the streamer has no recorder left
- ffprobe - Run from your `PATH`
    - If ffprobe is not found, no resolution is shown and a message is written to the Log tab
    - If ffprobe fails or takes too long, no resolution is shown and a message with the reason is written to the Log tab (see [Log tab](#log-tab))
- Upgrade quality - The resolution and frame rate that are read are the recording's quality, which the stream's quality is compared with (see [Upgrade quality](#upgrade-quality))

## Site color

- Site color - The color chosen for a site in its settings (see [Settings tab](#settings-tab))
    - Used for the site tag in Compact view, the site name in the site header on the Livestreams tab, the site tag on the Processes tab, and the domain of each streamer's URL in the normal view
    - A chosen color stays the same in every theme
    - When **Use theme color** is on, the site uses the accent color of the current theme, so it changes when you switch themes

## Disk space

- Disk space - Shows how much space is left on the drives that hold your output folders, so a drive that is filling up is noticed before a recording is lost
    - Shown in the header, to the left of the theme list, and in the Disk space card on the General page of the Settings tab (see [Settings tab](#settings-tab))
    - The free space of the relevant drive is also added to notifications (see [Notifications](#notifications))
- Drives - A drive is a drive letter on Windows, and a mount point on Linux and macOS (example: `C:` or `/mnt/recordings`)
    - Tracked automatically - Every drive that holds the global output folder, the output folder of a livestream site, or the download folder of a Posts site (see [Settings tab](#settings-tab) and [Posts site settings](#posts-site-settings))
        - A folder that does not exist yet is counted on the drive of its nearest existing parent folder
        - Several folders on the same drive are shown as one drive
        - The list follows the folders, so a drive appears when a folder is moved onto it, and goes away when no folder is left on it
    - Added drives - Drives you add yourself in the Disk space card, tracked in the same way, even when no output folder is on them
    - Only real drives are listed. System and virtual mounts are left out
    - A folder on a location that the system does not report as a drive, such as some network shares, is not shown
- Header chips - One chip for each connected drive, with the drive's name and its free space (example: `C: 412 GB free`). **Show percentage** adds the percentage free (example: `C: 412 GB 9% free`), and **Show free** removes or adds the word `free`
    - The name is the drive letter on Windows, and the last part of the mount path on other systems (example: `recordings`), or `/` for the root
    - Hovering a chip shows the full path, the free space out of the total size with its percentage, and the output folders that are on the drive
    - Up to six chips are shown. The rest are collapsed into a `+N` chip, and hovering it lists them
    - The chips follow the order of the list in the Disk space card, so the drives at the bottom of the list are the ones collapsed into the `+N` chip
    - An added drive that is not connected is not shown in the header
    - Nothing is shown while no drive is known
- Low space - A drive is low when its free space is under the percentage of its size or under the number of GB set in the Disk space card (see [Settings tab](#settings-tab))
    - A low drive's chip is shown in the warning color, and its tooltip says `Low on space`. The `+N` chip is shown in the warning color while a drive in it is low
    - A low drive is also shown in the warning color in the Disk space card
    - A notification about a low drive starts its free space line with a warning sign (see [Notifications](#notifications))
- Order - The drives can be moved up and down in the Disk space card, and the order is saved. It decides the order of the header chips and which drives are collapsed into the `+N` chip
    - A drive that is not in the saved order, such as a new one, is put at the bottom
- Display - **Show percentage** adds the percentage free to the header chips and to the notifications, and **Show free** adds or removes the word `free` in the same places. The Disk space card always shows the percentage free
- Refresh - The free space is read in the background about every 30 seconds, and right away when an output folder or an added drive changes, so the window stays responsive
- Sizes - Shown in MB, GB or TB, where 1 GB is 1024 MB. The free space is the space available to your user, which can be less than the total free space of the drive

## Setup notices

- Checked at launch and whenever settings change, and shown at the top of the Livestreams tab as **SETUP NEEDED**
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
    - An error that names a streamer, such as a channel that does not exist, only stops that streamer's live time from ending. The other streamers of the site carry on as normal. An error that does not name a streamer, and a check that fails without printing any error, stops it for every streamer of the site (see [Live time](#live-time))
    - A check that fails without printing any error is also reported
    - Checker arguments that contain neither `--dump-json` nor `--simulate` are shown the same way, with a hint to add one of them. The checker is not run until the arguments are fixed (see [Settings tab](#settings-tab))
    - The warning clears on the next check that succeeds
    - The Log tab gets one message when a problem appears or changes, and one when it clears
- Recording problems
    - When a recorder exits unexpectedly, jj-dlp writes one message to the Log tab and keeps retrying while the stream is live
    - Pausing, removing, quitting, and a recorder that exits normally are not problems
    - `RESTARTED N×` - Counts how many times the recorder started again after an unexpected exit, and stays until it is cleared (see [Restart warnings](#restart-warnings))
    - `INTERRUPT N×` and `ERROR N×` - Count the lines of the recorder's output that match the site's patterns, and stay until they are cleared (see [Output warnings](#output-warnings))
    - A failure that repeats with the same error is logged only the first time
    - Known setup problems show a warning with the fix under the streamer's row
        - ffmpeg is not installed
        - Firefox has no profile yet
        - The browser has its cookie database locked
    - The warning clears when a recording has run for 30 seconds or ends normally, or when you press **Re-check**
- Recording file problems
    - A recording whose file is missing or has stopped growing is shown as `STALLED N×` (see [Recording file checks](#recording-file-checks))
- If yt-dlp finds no cookies in the browser, a message is written to the Log tab once per live session, because the recording then runs without a login

## Recording file checks

- Recording file checks - Warn you when a recording's file is missing or stops growing, so a recording that is running but not saving is noticed
    - The checks only run once jj-dlp is sure which file the recording is writing (see below)
    - The check times are set per site on the Notifications page (see [Settings tab](#settings-tab)). Changes apply immediately, including to recordings that are already running
- Finding the file - jj-dlp finds the file in two ways, and only trusts it when both agree
    - ffmpeg - The output file on ffmpeg's command line
    - yt-dlp - jj-dlp adds `--print-to-file` options to the end of the recorder command, so yt-dlp writes the file name it plans to use to a small sidecar file (see [Sidecar files](#sidecar-files))
    - The two agree when yt-dlp's file name is the same as ffmpeg's output file. Relative names are resolved against the output folder
    - The recorder command needs `--no-part`. Without it ffmpeg writes to a temporary name, so the two never agree. The Site card shows a warning when `--no-part` is missing
- Not verified yet - Nothing is checked, and the **Open** button on the Processes tab is disabled
    - A recording that is still not verified after 2 minutes gets one warning in the Log tab
- Paths disagree - If ffmpeg's file and yt-dlp's file name still do not match after 5 seconds, the checks stay off for the rest of that recording
    - One warning with both paths is written to the Log tab. Nothing else is shown, and the recording carries on as normal
    - The next recording starts fresh
- Checks - Run for as long as the recorder runs, once the file is verified, and pause while the recorder is stopping
    - Warn if file doesn't exist after (seconds) - The file must exist this many seconds after it is verified. A missing file is `STALLED`, and so is a file that is removed later
    - Warn if file doesn't start growing after (seconds) - Growth is not checked until this many seconds after the file is verified
    - Warn if file stops growing for (seconds) - A file that has not grown for this many seconds is `STALLED`. The time counts from the last growth, or from the end of the growth delay if that is later. A file that exists but never grows is `STALLED` after both times together
    - Check for growth every (seconds) - How often jj-dlp looks at the file, so a problem can take up to this long to show
    - A file that cannot be read, such as one blocked by permissions, is treated like a missing file
- `STALLED N×` - A red chip shown next to `RECORDING` on the streamer row and in Compact view, and in the State column of the Processes tab, where the status dot also turns red, and where N is the number of times a recording of the streamer has stalled in the live time (see [Livestreams tab](#livestreams-tab), [Compact view](#compact-view) and [Processes tab](#processes-tab))
    - A warning is written to the Log tab when a recording becomes `STALLED`, with the reason
    - Count - Goes up by 1 each time a recording becomes stalled. It starts again from 1 at the first stall of a new live time (see [Live time](#live-time))
        - Clearing the chip does not reset it, so the next stall in the same live time carries on from the count it had
    - Staying - The chip stays after the file is growing again, and one message is written to the Log tab when the file recovers. It stays until it is dismissed or clears itself
    - Clicking the chip - Hides the chip for that streamer and keeps its count, without changing any other streamer (see [Livestreams tab](#livestreams-tab))
    - **Clear warnings** button - Hides every chip and keeps the counts (see [Livestreams tab](#livestreams-tab))
        - A chip that is hidden while the file is still stalled stays hidden until the file is growing again. A later stall is shown as normal
        - The Log tab messages are not changed, and the recording carries on as normal
    - Automatic clearing - When **Clear warnings after (minutes, 0 = never)** is above 0, the chip is cleared once that many minutes have passed since the file was growing again. The time does not run while a recording of the streamer is stalled. 0 turns it off, so the chip stays until you clear it. The count is kept
        - It is checked about every 30 seconds, so a chip can stay a little longer than the time set
    - The whole streamer row is colored red while a recording is stalled, and yellow once it has recovered but the chip is still shown. Red takes priority over the yellow color of `RESTARTED N×`
    - The **Warn** switch of the **Stalled** row turns the chip off for the site, and no chip is shown while all warnings are disabled globally. The checks still run, and the Log tab messages and the stalled notification are not changed (see [Notifications](#notifications))
    - The counts are not saved, so they are gone after jj-dlp restarts. Removing a streamer removes its count
- Open button - On each recorder card on the Processes tab. Opens the file the recorder is writing, in your default app
    - The `RECORDING` chip on the Livestreams tab, and `REC` in Compact view, do the same once the file is confirmed (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - Disabled until the file is verified and exists
    - If the file is not found when you click, a message is written to the Log tab
    - A file that is still being written only plays in players if its format allows it. A file with its index at the end, such as a regular mp4, may not play until the recording ends

### Sidecar files

- One small file per streamer, named after the streamer's id, in the `sidecars` folder inside the settings folder (see [Settings](#settings))
- Written by yt-dlp while a recording starts, and deleted when that streamer's last recorder is gone. The folder is emptied at launch
- The options jj-dlp adds are part of the recorder's full command line in the Process panel and in the `start:` line of the streamer's log file (see [Process panel](#process-panel-1) and [Log files](#log-files))

## Restart stalled recordings

- Restart stalled recordings - Stops a recording that has become `STALLED` and starts it again, so a recorder that has stopped writing its file does not carry on without producing one
    - Set per site with **Restart stalled recordings** in the Site card (see [Settings tab](#settings-tab)). There is no setting for a single streamer
    - It depends on the recording file checks, because a recording is only restarted once it is `STALLED` (see [Recording file checks](#recording-file-checks))
- When it restarts - As soon as a recording becomes `STALLED`, while the streamer is still being recorded
    - A streamer that is paused, switched off by its schedule, or about to be stopped for another reason is not restarted (see [Schedule](#schedule) and [Priority](#priority))
    - The recorder is stopped in the same way as for a paused streamer, so a recorder that does not exit on its own is stopped after a short wait, and the new recording starts once the old one is gone (see [Stopping behaviour](#stopping-behaviour))
    - The new recording is named by the auto-suffix setting (see [Auto-suffix](#auto-suffix))
    - It does not count as a restart (see [Restart warnings](#restart-warnings))
    - It does not depend on the **Warn** switch or the **Notify** switch of the **Stalled** row. The chip and the notification work as normal (see [Recording file checks](#recording-file-checks) and [Notifications](#notifications))
- Limit - At most 20 stalled recordings are restarted for a streamer during one live time (see [Live time](#live-time))
    - Once the limit is reached, no more are made until the live time starts over, the recording is left as it is, and the Log tab gets one message
    - The count is not saved, so it starts again from 0 after jj-dlp restarts
- Log tab - Messages are only written while **Restart stalled recordings** is on for the site (see [Log tab](#log-tab))
    - `<streamer>: restarting stalled recording (n/20): <reason>` - Each time a recording is restarted, with how many have been made in the live time out of the limit and the reason it stalled (example: `no file growth in 120s`)
    - A message that the limit has been reached - Once for each live time
- A change to the setting applies from the next stall

## Restart warnings

- Restart warnings - Show when a recorder keeps starting again, so a recording that exits over and over is noticed even when the streamer looks like it went offline
    - Shown as a `RESTARTED N×` chip on the streamer row and in Compact view (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - The settings are per site, on the Notifications page (see [Settings tab](#settings-tab))
- What counts as a restart - A new recorder starts for a streamer within **Consider a stream ended after (minutes)** of the previous recorder exiting on its own
    - An exit counts whether the recorder failed or exited normally, with exit code 0
    - The first recorder of a streamer is never a restart
    - An exit that is not followed by a new recorder is not a restart, so a stream that simply ends is not counted
    - A new recorder that starts after more than that many minutes is not counted
    - Pausing or removing a streamer, and quitting jj-dlp, are not counted. Pausing and then turning **Record** back on is not a restart either
    - Stopping a recorder at the end of a split, and the recorder of the new part, are not counted (see [Split recordings](#split-recordings))
    - It does not matter whether the recorder produced a file, or whether the checker reported the streamer offline in between
- Chip - `RESTARTED N×`, where N is the number of restarts
    - Shown once N reaches **Warning threshold (restarts)**. Lower counts are still kept, so the chip appears when the threshold is reached
    - The **Warn** switch of the **Restarts** row turns the chip off for the site, and no chip is shown while all warnings are disabled globally. The counts are still kept, so the chip is shown again when the switch is turned back on
    - Changing the threshold applies immediately
    - The whole streamer row is also colored yellow while the chip is shown, unless a recording is stalled, which colors it red
    - Stays until it is cleared. It is not cleared when the streamer goes offline, or when a recording runs without problems
- Clearing
    - **Clear warnings** button - Shown on the Livestreams tab, only while at least one `RESTARTED N×`, `INTERRUPT N×`, `ERROR N×` or `STALLED N×` chip is showing. Hides every chip, and keeps the restart, interrupt, error and stall counts of every streamer (see [Output warnings](#output-warnings) and [Recording file checks](#recording-file-checks))
    - Clicking the chip - Hides the chip of that streamer and keeps its count, without changing any other streamer. The chip is a glowing button (see [Livestreams tab](#livestreams-tab))
    - Automatic clearing - When **Clear warnings after (minutes, 0 = never)** is above 0, a streamer's chip is cleared once that many minutes have passed since its latest restart. 0 turns it off, so chips stay until you clear them
        - It is checked about every 30 seconds, so a count can stay a little longer than the time set
    - A cleared count is kept, so the next restart in the same live time carries on from it, and the chip is shown again once the count has reached the threshold. The next restart in a new live time starts the count again from 1 (see [Live time](#live-time))
- The counts are not saved, so they are gone after jj-dlp restarts
- Removing a streamer removes its counts

## Output warnings

- Output warnings - Show when a recorder's output has lines that match patterns, so an interruption or an error in a recording is noticed 
    - Shown as `INTERRUPT N×` and `ERROR N×` chips on the streamer row and in Compact view (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - The patterns, the **Warn** and **Notify** switches and the **Enable command output parsing** switch are set per site, on the Notifications page (see [Settings tab](#settings-tab))
- Enable command output parsing - Sends the recorder's output to be scanned for the patterns. Turn it off to stop the output from being sent at all and save resources, which also stops the chips from appearing
    - It is a separate switch from the patterns, so blank patterns no longer stop the output from being sent
    - While **Disable all notifications globally** and **Disable all warnings globally** are both on, it is turned off for every site, without changing the sites' own settings. The switch is shown off and cannot be changed, and the two pattern boxes are shown blank and greyed out and cannot be edited. It applies to recorders that start afterward (see [Notifications](#notifications))
    - While it is off, the **Interrupt patterns** and **Error patterns** boxes are shown blank and greyed out, and cannot be edited, and the **Interrupts** and **Errors** switches are greyed out. The patterns are kept, and shown again when it is turned on
    - The same output also fills the **Output** panel in the expanded recorder card on the Processes tab, so turning it off removes that panel (see [Process panel](#process-panel-1))
- Warn switches - The **Warn** switches of the **Interrupts** and **Errors** rows turn `INTERRUPT N×` and `ERROR N×` on or off for the site, without having to clear the patterns
    - While the **Warn** switch of a row is off, no lines are counted for its chip. No chip is shown while all warnings are disabled globally
    - The switches are greyed out and shown off while **Enable command output parsing** is off
    - While both the **Warn** and **Notify** switches of a row are off, no lines are matched for it, no Log tab message is written, and its pattern box is shown blank and greyed out and cannot be edited. The patterns are kept, and shown again when a switch is turned on
    - Chips that are already showing are not removed. Use **Clear warnings** to clear them (see below)
    - The switches apply from the next line of output, including to recordings that are already running
- Notify switches - The **Notify** switches of the **Interrupts** and **Errors** rows send a notification when a line matches that kind (see [Notifications](#notifications))
    - A **Notify** switch works without its **Warn** switch, so a notification can be sent without a chip being shown
- Patterns - **Interrupt patterns** decide `INTERRUPT N×`, and **Error patterns** decide `ERROR N×`
    - Separate the patterns with `|`
    - A line matches when it contains any of the patterns, in any upper or lower case, anywhere in the line. A pattern is plain text, not a regular expression
    - Spaces around a pattern, and empty entries, are ignored
    - While **Enable command output parsing** is on and the **Notify** or **Warn** switch of the row is on, the pattern box is required. It has a red border while it is blank, and **Save changes** is unavailable until it has a value
    - A blank field is only possible while both switches of its row are off, and it is not replaced by a default (see [Site defaults](#site-defaults))
- What is scanned - Every line of the recorder's stdout and stderr, which includes the lines ffmpeg prints, because yt-dlp passes them through
    - Progress lines that ffmpeg rewrites in place are not scanned
    - The checker's output is not scanned
    - Scanning does not depend on the log switches (see [Log files](#log-files))
- Chip - `INTERRUPT N×` or `ERROR N×`, where N is the number of matching lines
    - Shown as soon as N is 1. There is no threshold setting
    - `ERROR N×` is not shown for a streamer while `INTERRUPT N×` has also been triggered in the same live time. Dismissing or clearing the `INTERRUPT N×` chip does not bring it back, and the error count is still kept. It is shown again at the first matching error line of a new live time
    - Each matching line adds 1, so a single interruption can print many matching lines and raise N by more than 1
    - The counts are kept for each streamer when the recorder starts again, and when the streamer goes offline. A count is also kept when its chip is cleared, and starts again from 1 only at the next matching line of that kind in a new live time
    - The whole streamer row is also colored yellow while either chip is shown, unless a recording is stalled, which colors it red
- Log tab - One message is written the first time each kind matches for a streamer in a live session. Later matches in the same live session only change the count (see [Log tab](#log-tab))
    - The message ends with the pattern that matched, in lower case (example: `recorder output matched an error pattern: packet corrupt`). When a line matches several patterns of the same kind, the first one in the field is shown
    - Each kind is written again in the next live session
- Clearing
    - **Clear warnings** button - Shown on the Livestreams tab, while any warning chip is showing. Hides every chip, and keeps the interrupt and error counts of every streamer, together with the restart counts and the stall counts (see [Restart warnings](#restart-warnings))
    - Clicking a chip - Hides the chip of that kind for that streamer only, and keeps its count. The other kind and the other streamers are not changed. Each chip is a glowing button (see [Livestreams tab](#livestreams-tab))
    - Automatic clearing - When **Clear warnings after (minutes, 0 = never)** is above 0, the chip of a kind is cleared once that many minutes have passed since its latest matching line, and the other kind is not changed. 0 turns it off, so chips stay until you clear them
        - It is checked about every 30 seconds, so a count can stay a little longer than the time set
    - A cleared count is kept, so the next matching line of that kind in the same live time carries on from it and shows the chip again. A matching line in a new live time starts the count again from 1 (see [Live time](#live-time))
- Changing the patterns applies from the next line of output, including to recordings that are already running
    - Turning **Enable command output parsing** on or off applies to recorders that start afterward. A recorder that is already running keeps its earlier setting until it starts again
- The counts are not saved, so they are gone after jj-dlp restarts
- Removing a streamer removes its counts

## Notifications

- Notifications - Tell you when a streamer goes live, and optionally when a recording becomes `STALLED` or `RESTARTED N×`, or when a recorder's output matches an interrupt or error pattern
    - The settings are per site, on the Notifications page of the Settings tab (see [Settings tab](#settings-tab))
    - Each streamer can follow the site's **Enable Notifications** switch, or turn notifications on or off for itself (see [Streamer settings](#streamer-settings))
    - Channels - **Desktop Notifications** and **Mobile Notifications (ntfy)** are turned on separately. They can only be changed while the site's **Enable Notifications** switch is on, and are shown off while it is off
        - Desktop Notifications - A native toast on Windows, and `notify-send` on Linux. macOS has no desktop notifications, so only ntfy is used there
        - Mobile Notifications (ntfy) - Sends the notification to the topic in **ntfy topic (example: j73hd92xl)**, so it reaches the ntfy app on your phone. The notification is sent through `ntfy.sh`. A site with a blank topic uses the global ntfy topic (see below)
        - Tapping an ntfy notification opens the streamer's stream URL
        - Nothing is sent when both channels are off, and the **Notify** switches of the site's notifications and warnings list are greyed out (see [Settings tab](#settings-tab))
        - The **ntfy topic** box cannot be edited, and has no hint text, while the site's **Enable Notifications** switch or **Mobile Notifications (ntfy)** is off
    - Global switches - Set on the **General** sub-page of the Notifications page (see [Settings tab](#settings-tab))
        - **Disable all notifications globally** - Sends no notification of any kind for any site, Livestreams or Posts, without changing the sites' own settings. The notification controls of every site are greyed out while it is on
        - **Disable all warnings globally** - Shows no warning chip for any site, without changing the sites' own settings. The **Warn** controls of every site are greyed out while it is on. Notifications are not affected
        - While both switches are on, **Enable command output parsing** is turned off for every site as well, without changing the sites' own settings (see [Output warnings](#output-warnings))
        - Global ntfy topic - The topic used by every site, Livestreams or Posts, whose own ntfy topic is blank. A site's own topic is used when it has one
            - The site's topic box shows the global topic as hint text while it is blank. Nothing is shown when no global topic is set
            - A site can be saved with a blank topic while the global topic is valid, and **Send test** uses the global topic in the same way
- What is sent
    - Went live - Sent when a streamer's live time starts (see [Live time](#live-time)). Needs the **Notify** switch of the **Live** row on the site
        - A streamer who goes offline and comes back sooner than **Consider a stream ended after (minutes)** keeps the same live time, so no second notification is sent
        - Pausing and then turning **Record** back on does not start a new live time, so it sends nothing
        - A second went live notification is sent when an intro delay ends and recording starts (see [Intro delay](#intro-delay))
        - A second went live notification is also sent when a streamer that was waiting for a free recording slot starts recording (see [Priority](#priority))
        - The title is `<streamer> is live`, and the body is described below
    - Stalled - Sent once when a recording becomes `STALLED` (see [Recording file checks](#recording-file-checks)). Needs the **Notify** switch of the **Stalled** row on the site
        - It is not sent again while the recording stays `STALLED`, and hiding the chip with **Clear warnings** does not send anything
        - The title is `<streamer>: recording stalled`, and the body shows the site
    - Restarted - Sent when the restart count reaches **Warning threshold (restarts)** (see [Restart warnings](#restart-warnings)). Needs the **Notify** switch of the **Restarts** row on the site
        - Later restarts are not sent while the chip is showing. The first restart after the chip has been cleared is sent again, because the chip is shown again
        - The title is `<streamer>: recording restarted`, and the body shows the site and the number of restarts
    - Interrupt - Sent when a line of the recorder's output matches an interrupt pattern (see [Output warnings](#output-warnings)). Needs the **Notify** switch of the **Interrupts** row on the site, and **Enable command output parsing**
        - It follows the cooldown, so a recording that prints many matching lines sends one notification for each cooldown period
        - The title is `<streamer>: recording interrupted`, and the body shows the site
    - Error - Sent when a line of the recorder's output matches an error pattern. Needs the **Notify** switch of the **Errors** row on the site, and **Enable command output parsing**
        - The title is `<streamer>: recording error`, and the body shows the site
- Free space line - A last line added to every notification, with the free space left on the drive of the output folder the notification is about (example: `💾 412 GB free on D:`) (see [Disk space](#disk-space))
    - The line follows the **Show percentage** and **Show free** switches (example: `💾 412 GB 9% free on D:`)
    - Went live, stalled, restarted, interrupt and error notifications use the folder the streamer's recordings are saved in. New posts notifications use the site's download folder. **Send test** uses the output folder in the Site card as it is now
    - When the drive is low on space, the line starts with a warning sign instead (example: `⚠️ 12 GB free on D:`)
    - The space is read when the notification is sent. The line is left out when the drive cannot be found
    - It is sent to the desktop and ntfy channels, in the same way as the rest of the notification
- Went live notification body - Up to four lines, in this order
    - Status - A colored dot and `Status:`, followed by `Recording` or `Not recording`
        - `Recording (confirmed)` with a red dot - The streamer is being recorded, and **Wait for confirmed growth** is on, so the file was seen growing first
        - `Recording (unconfirmed)` with a red dot - The streamer is being recorded, and **Wait for confirmed growth** is off, so the notification is sent as soon as the streamer is seen live
        - `Not recording` with a yellow dot - The streamer is not being recorded
    - Reason - Shown only when the status is `Not recording`
        - `Reason: Disabled` is shown when **Record** is turned off for the streamer (see [Livestreams tab](#livestreams-tab))
        - `Reason: Intro Delay` is shown while the streamer's intro delay has not ended (see [Intro delay](#intro-delay))
        - `Reason: Lower priority` is shown when the recording limit is reached and no streamer that is recording has a lower priority (see [Priority](#priority))
    - Warning - A yellow dot and `Warning: evicted <streamer>`, with one line for each streamer that was stopped to make room for this one. Only shown when a streamer was stopped (see [Priority](#priority))
    - Site - `Site:` and the name of the streamer's site
- Wait for confirmed growth - Holds the went live notification until the recording file is seen growing (see [Recording file checks](#recording-file-checks))
    - Only applies to the went live notification, and only while the streamer is being recorded. A streamer with **Record** turned off, or one that is waiting out an intro delay, has nothing to confirm, so the notification is sent right away
    - The notification is dropped, and not sent later, when the stream ends before the file is seen growing, or when **Record** is turned off for the streamer while it is held
    - Stalled, restarted, interrupt and error notifications are never held
- Notification Cooldown - The minutes that must pass before the same kind of notification is sent again for the same streamer
    - Went live, stalled, restarted, interrupt and error notifications have separate cooldowns, and each streamer has its own
    - The time is counted from the last notification that was sent. A notification that was held, dropped or skipped does not start it
    - The went live notification that follows an intro delay ignores the cooldown (see [Intro delay](#intro-delay))
    - 0 turns the cooldown off
    - The cooldown times are not saved, so they start again after jj-dlp restarts
- Notification Timeout - How many seconds the desktop notification is shown. It is a request, and the system decides
    - Windows - The nearer of the system's short and long toast times is used
    - Linux - Passed to `notify-send`. Some notification daemons ignore it
    - ntfy notifications have no timeout
- Already live at launch - A streamer who is already live at their first check after jj-dlp starts sends a went live notification only while **Notify when already live at launch** is on for the site
    - Applies to a streamer added while jj-dlp is running too
    - With it off, the first check sends nothing
    - A live time that was saved and is still open after a restart is not a new live time, so it sends nothing either way (see [Live time](#live-time))
- Send test - The **Send test** button on the Livestreams sub-page of the Notifications page sends a test notification to the channels that are turned on there
    - It uses the values on the page as they are now, so it can be used before they are saved
    - It ignores the **Enable Notifications** switch, the cooldown and the global switch that disables notifications
    - Unavailable while **Desktop Notifications** and **Mobile Notifications (ntfy)** are both off, or while **Mobile Notifications (ntfy)** is on and neither the site's topic nor the global topic is valid
    - One message is written to the Log tab, and a message is added if a channel fails (see [Log tab](#log-tab))
- Problems
    - A failed desktop or ntfy notification writes a message to the Log tab. The notification is not sent again
    - Linux - If `notify-send` is not installed, one message is written to the Log tab per session. **Send test** always shows it

## Recording subfolders

- Recording subfolders - Saves each recording in a subfolder, or nested subfolders, of the output folder instead of directly in it
    - Set per site in the Site card, and per streamer in the streamer's settings (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
    - Layouts - Shown with `\` on Windows and `/` on other systems. The examples are for the site `Twitch` and the streamer `Emiru`
        - `No subfolder` - `<output folder>\<filename>.mp4`
        - `streamer\` - `<output folder>\Emiru\<filename>.mp4`
        - `site\` - `<output folder>\Twitch\<filename>.mp4`
        - `streamer\site\` - `<output folder>\Emiru\Twitch\<filename>.mp4`
        - `site\streamer\` - `<output folder>\Twitch\Emiru\<filename>.mp4`
    - Default - `streamer\`, for new sites and for sites that were saved before this setting existed
    - Inherit - A streamer can use its own layout, or **Inherit** to use the layout of its site. A streamer that inherits follows the site whenever the site's layout changes
    - Folder names - The site name and the streamer name are used as the folder names
        - Characters that cannot be used in a folder name (`<>:"/\|?*` and control characters) are replaced with `_`
        - Trailing dots and spaces are removed
        - A name Windows does not allow, such as `CON`, `NUL`, `COM1` or `LPT1`, gets a `_` in front, on every system
        - A blank name becomes `_`
- Effective path - Shown in the Site card and in the streamer's settings, so you can see where recordings will be saved
    - Example: `Effective path: C:\Users\owner\Videos\jj-dlp\Twitch\Emiru\<filename>.mp4`
    - The output folder is the site's output folder, or the global one if the site has none, shown as a full path
    - `<filename>.mp4` is a placeholder. The real file name comes from the `--output` template in the recorder command
    - The Site card shows `<streamer>` in place of the streamer's name, because it applies to every streamer of the site. It follows the site name and output folder as you edit them
    - The streamer's settings show the streamer's own name, and the layout that streamer uses, including an inherited one
- How it works - jj-dlp adds an `--output` option to the end of the recorder command when a recording starts, with the subfolders in front of the `--output` template of the command
    - If the recorder command has no `--output`, yt-dlp's default template, `%(title)s [%(id)s].%(ext)s`, is used
    - `No subfolder` adds nothing
    - Only a plain `--output` template is changed. An `--output` for another type of file, such as `thumbnail:`, is left as it is
    - A recorder command typed with the short `-o` is read the same way as `--output`
    - The subfolders are created by yt-dlp when the recording starts
    - The recording file checks follow the file into the subfolders (see [Recording file checks](#recording-file-checks))
    - The added option is part of the recorder's full command line in the Process panel and in the `start:` line of the streamer's log file (see [Process panel](#process-panel-1) and [Log files](#log-files))
    - From the second part of a recording, the `_partN` suffix is added to the template too, with or without subfolders (see [Split recordings](#split-recordings))
- Absolute `--output` path - If the `--output` template of the recorder command is an absolute path (example: `D:\recordings\%(title)s.%(ext)s`), no subfolders are added
    - A warning is shown under the Effective path in the Site card and in the streamer's settings, unless the layout is `No subfolder`
- A layout change applies to the next recording. A recording that is already running keeps its folder
- Changing a layout, or renaming a site or streamer, does not move recordings that are already saved

## Split recordings

- Split recordings - Starts a new file after a set number of minutes, so a long stream is saved as several parts
    - Set per site in the Site card, and per streamer in the streamer's settings (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
    - Split length - The minutes each part runs before the next part starts, counted from when that part's recorder started
    - Inherit - A streamer uses the site's switch and length, and follows the site whenever they change
    - On - Splits this streamer even when the site's switch is off. It uses the streamer's own length, which must be entered
    - Off - Never splits this streamer
    - Used by the GUI and `--no-gui`. `--engine-only` has no settings layer, so it ignores splitting and suffixes (see [Command-line modes](#command-line-modes))
- Part names - Part 2 and up have `_partN` added before the file extension of the `--output` template (example: `<filename>_part2.mp4`)
    - The first part has no suffix while it is the only part. When part 2 is recording, the first file is renamed to `_part1`, so the parts sort together
    - The suffix is added in the same subfolder, and to an absolute `--output` path too (see [Recording subfolders](#recording-subfolders))
    - The first file is renamed after the recorder that wrote it has exited. It is left as it is when its path was never verified, or when a file with the `_part1` name already exists, and a message is written to the Log tab
- How a split works - The next part starts while the current part keeps recording, so nothing from the stream is missed
    - The old recorder is stopped once the new part's file is seen growing (see [Recording file checks](#recording-file-checks))
    - Both recorders are shown on the Processes tab until the old one has exited (see [Processes tab](#processes-tab))
    - Stopping the old recorder does not count as a restart, and does not show `STALLED` (see [Restart warnings](#restart-warnings))
    - The new part has 30 seconds plus two **Check for growth every (seconds)** times of the site's recording file checks to be seen growing. If it is not, or its recorder exits first, the new recorder is stopped, the current recording carries on and the split is tried again once after 1 minute. If that try fails too, the split is not tried again for the rest of that live time
    - The recorder command needs `--no-part`, because the new part cannot be seen growing without a verified file. Without it every split fails, and after the one retry splitting stops for that live time
- Changing - Turning splitting on, off or changing the length applies to recordings that are already running, counted from when the current part started
    - A split that has already started is finished
    - Pausing or removing a streamer, or the stream ending, stops the recorders and any split in progress
- Part numbers are saved with the live time, so they carry on after jj-dlp restarts. A new live time starts again at the first part (see [Live time](#live-time))
- Log tab - A message is written when the next part starts, when it is recording, when it is not seen growing in time, and when a file is renamed or cannot be renamed (see [Log tab](#log-tab))

### Auto-suffix

- Auto-suffix - Names a recording that starts again during the same live time with the next `_partN`, so the new recording does not use the name of the file the earlier one wrote
    - Set per site with **Name restarted recordings _partN**, and per streamer with **Inherit**, **On** or **Off** (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
    - Applies whenever a new recorder starts for a streamer who has already recorded during the same live time (see [Live time](#live-time))
        - After a recorder exits unexpectedly or normally
        - After **Record** is turned off and on again
        - After jj-dlp restarts while the stream is still live
    - The first file is renamed to `_part1` once the new recording's file is seen growing, in the same way as for a split (see [Split recordings](#split-recordings) and [Recording file checks](#recording-file-checks))
        - A new recorder that ends without a file being seen growing leaves the first file with its own name, so a lone `_part1` file is not left behind
    - With it off, the new recorder uses the same file name as before
    - A recorder that fails to start does not use up a part number. The number only moves on once a recorder has actually started, so the first file saved after failed attempts is not given a higher number than it needs
    - A recorder that ends without having created a file does not use up a part number either. The next recording reuses the number, so no `_partN` is skipped. A recording whose file was created always moves the number on, so the file is never overwritten
    - Closing the window, or quitting jj-dlp in any other way, does not use up a part number either. The recording that starts after jj-dlp is opened again carries on from the next number, so no `_partN` is skipped
    - A streamer whose recordings are split, or whose intro is split, still gets the suffix after a restart when auto-suffix is off, so a new part never uses the name of an earlier one

## Upgrade quality

- Upgrade quality - Restarts a recording once when the stream starts offering a higher quality than the one being recorded, so a stream that goes live at a lower quality and later rises is not recorded at the lower quality for the rest of the stream
    - Set per site with **Upgrade quality when available** in the Site card, and per streamer with **Inherit**, **On** or **Off** (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
    - Inherit - A streamer uses the site's switch, and follows the site whenever it changes
    - On and Off - Apply to this streamer only, whatever the site's switch is
    - Only applies to livestream recordings, not to Posts
    - Used by the GUI and `--no-gui`. `--engine-only` has no settings layer, so it ignores it (see [Command-line modes](#command-line-modes))
- Stream quality - Read from the checker's output at every check, so it needs `--dump-json` in the site's checker arguments (see [Settings tab](#settings-tab))
    - It is the resolution of the format yt-dlp would record, with its frame rate when the output has one
    - A checker that uses `--simulate` reports no quality, so nothing is upgraded for that site
- Recording quality - The resolution and frame rate that ffprobe reads from the recording file once it is confirmed (see [Resolution](#resolution) and [Recording file checks](#recording-file-checks))
    - Each new recorder is read again, so the recording quality always follows the file that is being written now
    - Nothing is upgraded while the recorder has no confirmed file
- Armed - The stream quality is only trusted once it has been matched to a real file, because the two can disagree
    - Armed when the resolution of the recording matches a stream quality that the checker has reported during the same live stream, including checks made before the recording started
    - Frame rate - When both the checker output and ffprobe give a frame rate, and the two are within 1 frame per second, the frame rate is compared as well. Otherwise only the resolution is compared
    - Not armed - No upgrade is made while it is not armed. A later check that matches the recording arms it
    - One message is written to the Log tab when it is still not armed 60 seconds after the recording was first read (see [Log tab](#log-tab))
- When it upgrades - Once armed, a check that reports a higher quality than the recording restarts the recording
    - Higher - A larger picture, whatever the frame rate, so a larger resolution at a lower frame rate is an upgrade. When the frame rate is compared, the same size at a frame rate more than 1 frame per second higher is an upgrade too
    - A lower quality, or the same quality, never restarts a recording
    - The recorder is stopped, in the same way as when a streamer is paused, and a new recorder starts right away, which picks the best quality yt-dlp offers (see [Stopping behaviour](#stopping-behaviour))
    - It does not count as a restart, and does not show `STALLED` (see [Restart warnings](#restart-warnings))
    - A streamer that is about to be stopped for another reason, such as its priority, is not restarted (see [Priority](#priority))
    - One message is written to the Log tab with the old and the new quality
- Once per live stream - A recording is upgraded once for each live time, even when the new recording is not at the higher quality, so a recording is never restarted over and over (see [Live time](#live-time))
    - It is counted when the restart starts
    - It is saved, so it carries on after jj-dlp restarts. A new live time can be upgraded again
- File name - The new recording follows the same rules as any other recording that starts again (see [Auto-suffix](#auto-suffix) and [Split recordings](#split-recordings))
    - With auto-suffix on, the new recording is the next `_partN`
    - With auto-suffix off and no splitting, the new recording uses the same file name as before
- Log tab - Messages are only written while upgrade quality is on for the streamer (see [Log tab](#log-tab))
    - `upgrading quality` - When a recording is restarted for a higher quality, with the old and the new quality (example: `1280x720@30 -> 1920x1080@60`)
    - `quality upgrade not armed` - When it is still not armed after 60 seconds, with the stream quality and the recording quality that did not match
    - `quality upgrade inactive` - Once for each live stream, when a check of a live stream reports no resolution, or when ffprobe cannot read the recording. The reason for an ffprobe problem is also written, as described in [Resolution](#resolution)
    - Nothing is written when the recording file is never confirmed
- A change to the setting applies from the next check or the next time the recording is read

## Quick check

- Quick check - Checks a streamer again on its own shortly after its recording ends, so a stream that ends and then starts again is picked up without waiting for the site's next check
    - Set per site with **Quick check ended streams** and **Quick check after (seconds)** in the Site card, and per streamer with **Inherit**, **On** or **Off** (see [Settings tab](#settings-tab) and [Streamer settings](#streamer-settings))
    - Inherit - A streamer uses the site's switch, and follows the site whenever it changes
    - On and Off - Apply to this streamer only, whatever the site's switch is
    - The delay is set per site, and is used by every streamer of the site that has quick check on
    - Meant for sites where a check takes a long time, because the more streamers a site has, the longer a check takes, and a streamer that starts again right after ending would otherwise wait for the whole check
    - Only applies to livestream recordings, not to Posts
    - Used by the GUI and `--no-gui`. `--engine-only` has no settings layer, so it ignores it (see [Command-line modes](#command-line-modes))
- When it runs - Once the delay has passed after a recording ends
    - Armed - A recording arms it once its file is confirmed (see [Recording file checks](#recording-file-checks)). A recording whose file is never confirmed does not arm it
    - A recording that ends on its own counts, whether the recorder failed or exited normally. Pausing or removing a streamer, a schedule or a priority switching it off, a quality upgrade and quitting jj-dlp do not, because the recorder is stopped on purpose (see [Restart warnings](#restart-warnings))
    - With split recordings, it starts when the streamer's last recorder has exited (see [Split recordings](#split-recordings))
    - The delay is counted from when the recorder exits
    - Once - One quick check is made for each recording that ends. A new recording has to be confirmed before it can arm another one, so quick checks never repeat on their own
    - A new recording that starts before the check is due, from the site's regular check or by turning **Record** on, cancels it
    - Skipped when it is due - The check is not made when the streamer is paused or removed, the site's checking or the streamer's checking is paused, quick check has been turned off for the streamer, or the streamer is already shown as live or recording
- The check - Runs the site's checker command with only that streamer's URL (see [Settings tab](#settings-tab))
    - Runs alongside the site's regular check and does not wait for it
    - A site runs one quick check at a time. When several streamers are due together, they wait in turn, and a streamer waits in the queue only once
    - A streamer that is seen live is recorded as normal, so the intro delay and the priority still apply (see [Intro delay](#intro-delay) and [Priority](#priority))
    - A streamer that is not seen live, or a check that fails, changes nothing more. It is not tried again, and the site's regular checks carry on as normal
    - It does not change the check time or the problem warning shown in the site header (see [Livestreams tab](#livestreams-tab))
    - A check that started earlier never replaces the result of a check that started later, so a slow regular check that started before a quick check does not mark the streamer offline again
- New streamers - A streamer that is added is checked once with the same one-streamer check, and the site's regular check does not run at once because of it
    - The streamer joins the site's regular checks from their next scheduled run
    - Made whether or not quick check is on for the streamer, and it does not count towards the limit below
    - Also made for a streamer added by editing the config file by hand
    - Not made when the site's checking or the streamer's checking is paused. A site that is created by adding its first streamer checks that streamer in its own first check
    - A message that the check failed is written to the Log tab as `<streamer>: quick check failed: <reason>`
- Limit - At most 20 quick checks are made for a streamer during one live time (see [Live time](#live-time))
    - A check counts when it is made, whether or not the streamer is seen live
    - Once the limit is reached, no more are made until the live time starts over, and the Log tab gets one message
    - The count is not saved, so it starts again from 0 after jj-dlp restarts
- Log tab - Messages are only written while quick check is on for the streamer (see [Log tab](#log-tab))
    - `<streamer>: running a quick check to verify stream has ended (n/20)` - Each time a quick check is made, with how many have been made in the live time out of the limit
    - A message that the limit has been reached - Once for each live time
    - A message that the check failed, with the reason - When the checker cannot be run or reports an error
- A change to the setting applies from the next recording that ends, and turning it off also cancels a quick check that is waiting

## Intro delay

- Intro delay - Holds back recording for a set number of minutes after a streamer goes live, so the start of a stream is not recorded
    - Set per streamer in the streamer's settings (see [Streamer settings](#streamer-settings))
    - A delay of 0 minutes, or a blank minutes box, means no delay
    - Split the intro - Instead of waiting, a streamer can be recorded right away and have the intro split into its own file (see below)
    - Used by the GUI and `--no-gui`. `--engine-only` has no settings layer, so it ignores the delay and records every enabled streamer as soon as they are seen live (see [Command-line modes](#command-line-modes))
- When the delay ends - The delay is counted from the start of the streamer's live time (see [Live time](#live-time))
    - Recording starts as soon as the delay ends, without waiting for the next check
    - A streamer who goes offline and comes back sooner than **Consider a stream ended after (minutes)** keeps the same live time, so the delay is not counted again
    - A recorder that exits unexpectedly after the delay and starts again does not wait again
    - When jj-dlp starts, the delay is counted from the live time that was saved. If none was saved, it is counted from the first check that sees the streamer live
    - A stream that has already been live for longer than the delay when it is first seen starts recording right away
- While waiting
    - Livestreams tab - `LIVE` and the live time are shown, but not `RECORDING`, and no card is shown on the Processes tab. The status dot glows green while **Record** is on (see [Livestreams tab](#livestreams-tab))
    - Compact view - `LIVE` is shown without `REC` (see [Compact view](#compact-view))
    - Pausing a streamer during the delay means nothing is recorded. Turning **Record** back on while the delay is still running keeps waiting, and turning it on after the delay has ended starts recording right away
    - The stream ending before the delay is over means nothing is recorded
- Changing the delay
    - Applies as soon as the streamer's settings are saved, to a streamer that is waiting. Shortening the delay or turning it off starts recording right away if the new end has already passed
    - Never stops a recording that is already running, so turning the delay on or making it longer only affects the next time recording would start
- Split the intro - **Split the intro into its own file instead of waiting** records from the moment the streamer goes live, and splits after the delay's minutes (see [Split recordings](#split-recordings))
    - Needs **Intro Delay** on and minutes above 0. Otherwise it does nothing
    - The intro is saved as the first part and the rest of the stream as the second part. The first file is renamed to `_part1`
    - There is no waiting, so `Intro Delay` and the second went live notification are not used
    - It splits only once. After that there are no more timed splits for that live time, even when splitting is on for the streamer or the site
    - It works without splitting being turned on for the site or the streamer
    - If the recording restarts before the intro has been split, the intro is not split (see [Auto-suffix](#auto-suffix))
    - Turning it on for a streamer that is waiting starts recording right away
- Notifications - Two went live notifications can be sent for one stream (see [Notifications](#notifications))
    - Waiting - Sent when the streamer is first seen live, right away, with `Not recording` and `Reason: Intro Delay`. It is sent even when **Wait for confirmed growth** is on, because there is nothing to confirm yet
    - Recording - Sent when the delay ends and recording starts, with the status described in [Notifications](#notifications). It ignores the cooldown, and follows **Wait for confirmed growth** like any other went live notification
    - The second notification is only sent when the first one was sent, so a streamer who was already live at launch gets neither unless **Notify when already live at launch** is on, and a site or streamer with notifications turned off gets neither
    - The second notification is not sent when the stream ends, or **Record** is turned off, before the delay ends

## Schedule

- Schedule - Switches a streamer on and off at set times, so a streamer is only recorded during the times you choose
    - Set per streamer in the streamer's settings (see [Streamer settings](#streamer-settings))
    - Switching a streamer on or off is the same as turning **Record** on or off, so a streamer that is switched off is paused (see [Livestreams tab](#livestreams-tab))
    - The times are your local time
    - Used by the GUI and `--no-gui`. `--engine-only` has no settings layer, so it ignores schedules (see [Command-line modes](#command-line-modes))
- Modes
    - **One-off** - Runs once, from a **Start** date and time to an **End** date and time
    - **Recurring** - Repeats every week on the days you select, from a **From** time to a **To** time
        - A **To** time that is earlier than the **From** time makes the window run past midnight, and it ends on the day after the selected day
        - The days are shown as `Mon` to `Sun` buttons, and a selected day is highlighted. Click a day to select or clear it
- Pickers - Dates and times are chosen with pickers, so nothing is typed
    - Each date and time is a button. Click it to open the picker, then choose a value and confirm it, or cancel to close the picker without changing anything
    - Dates are chosen from a calendar, and times are chosen in 24-hour format
- When it switches - A schedule switches the streamer on once its start has passed, and off once its end has passed
    - The schedule is checked about every 10 seconds, when jj-dlp starts, and right after the streamer's settings are saved
    - Each start and each end is applied once. After that the schedule does nothing until the next start or end, so turning **Record** on or off by hand is not undone, and holds until the next start or end
    - When a start and an end have both passed without being applied, such as when the schedule is first turned on, or when jj-dlp was closed while one passed, only the latest one is applied. A schedule that is turned on while its window is open switches the streamer on, and one turned on after its window has ended switches the streamer off
    - Switching a streamer off stops its running recorder, in the same way as pausing it (see [Stopping behaviour](#stopping-behaviour))
    - Switching a streamer on starts recording right away if the stream is still live, unless the streamer's intro delay has not ended yet (see [Intro delay](#intro-delay))
    - Turning the schedule off, or changing it, never switches the streamer on or off by itself, apart from what a start or an end that has newly passed calls for
- Chip - `SCHEDULED` is shown on the streamer row, and `SCH` in Compact view, while the schedule is turned on (see [Livestreams tab](#livestreams-tab) and [Compact view](#compact-view))
    - A streamer that the schedule has switched off also shows `PAUSED`
- Saving - **Save changes** is unavailable while the schedule is turned on and it is not complete
    - One-off - A start or an end has not been chosen, or the end is not after the start
    - Recurring - No day is selected, a time has not been chosen, or the **From** and **To** times are the same
    - The buttons that need a value are shown with a red border, and a short message is shown under the schedule
    - A schedule that is turned off can be left incomplete
- Log tab - One message, `Schedule: enabled <streamer>` or `Schedule: disabled <streamer>`, is written when a schedule changes whether the streamer is switched on (see [Log tab](#log-tab))
    - Nothing is written when the streamer was already in the state the schedule calls for
- The schedule and the time it last switched the streamer on and off are saved, so they carry on after jj-dlp restarts (see [Settings](#settings))

## Priority

- Priority - Decides which streamers are recorded first when a limit on how many are recorded at once is set, so the streamers you care most about are recorded
    - Set on the **Priority** page of the Settings tab (see [Settings tab](#settings-tab))
    - One list for every streamer of every site
    - Used by the GUI and `--no-gui`. `--engine-only` has no settings layer, so it ignores the limit and the priority (see [Command-line modes](#command-line-modes))
- Recording limit - The most streamers that are recorded at once
    - A blank box, or 0, means no limit
    - A streamer counts while it is recording. A streamer that is waiting out an intro delay does not count until its recording starts (see [Intro delay](#intro-delay))
    - A streamer counts once, even while two of its recorders run at the same time during a split (see [Split recordings](#split-recordings))
    - A streamer that is paused, or is not live, never uses a slot
- Priority list - Every streamer of every site, one row each, with its place number, its site as a tag in its site color and its name (see [Site color](#site-color))
    - The top of the list has the highest priority, and the bottom has the lowest
    - A streamer that is added is put at the bottom. A streamer that is removed and added again is also put at the bottom
    - Click a row to select it. **Move up** and **Move down**, shown beside the **PRIORITY** title, move the selected streamer one place. They are unavailable while no row is selected, and at the top or bottom of the list
    - The selection stays on the streamer after it is moved
    - The **PRIORITY** title with **Move up** and **Move down** stays in place while the list scrolls, so only the rows move
    - The list scrolls by itself when a moved streamer would leave the visible part of the list, so the selected row stays in view
- Always record - A switch on each row of the list
    - A streamer with it on is recorded even when the limit is reached, and is never stopped to make room for another streamer
    - It still counts toward the limit, so recording it can take the number above the limit, and no other streamer starts until the number is back under it
    - It can still stop lower priority streamers to make room, when the limit is reached
    - Turning it on moves the streamer to the top of the list
- Saving - Edits are held as a draft until you click **Save changes**. Nothing is applied while you edit
    - An `⚠ Unsaved changes. Press Save changes to apply them.` message is shown in the warning color in the Recording limit card, beside the **Revert** and **Save changes** buttons, while there are edits. It goes away once they are saved or reverted
    - **Save changes** - Saves the order, the **Always record** switches and the limit. Unavailable while there are no edits. Pressing Enter in the limit box does the same
    - **Revert** - Discards your edits. Unavailable while there are no edits
    - A streamer that is added while you have unsaved edits is shown at the bottom, and a streamer that is removed disappears from the list. The rest of your edits are kept
    - Saving applies to recordings that are already running. A recording that no longer fits the order and the limit is stopped (see below)
- When a streamer goes live at the limit
    - When a streamer with a lower priority is recording, the lowest priority one is stopped and the new streamer starts recording. The went live notification of the new streamer has a `Warning` line for each streamer that was stopped (see [Notifications](#notifications))
    - When no streamer with a lower priority is recording, the new streamer is not recorded. The went live notification has `Not recording` and `Reason: Lower priority`
    - A streamer that is not recorded for its priority shows `LIVE` and the live time, but not `RECORDING`, and has no card on the Processes tab (see [Livestreams tab](#livestreams-tab))
    - It starts recording as soon as a slot is free, without waiting for the next check. When several streamers are waiting, the highest priority one starts first
    - A second went live notification is sent when it starts recording. It ignores the cooldown and follows **Wait for confirmed growth**, like the one after an intro delay (see [Intro delay](#intro-delay))
    - A streamer that was stopped to make room waits in the same way, and sends no notification of its own
    - A streamer that is stopped for its priority is stopped like a paused streamer (see [Stopping behaviour](#stopping-behaviour)). It does not count as a restart, and does not show `STALLED` (see [Restart warnings](#restart-warnings))
    - The new recording starts without waiting for the stopped one to exit, so one more than the limit can run for a short time
    - When a streamer records again during the same live time, the new file gets the next `_partN` while auto-suffix is on (see [Auto-suffix](#auto-suffix))
- When settings change - Saving a lower limit, or a new order, stops the recordings that no longer fit, and the streamers stopped wait as described above
    - Adding or removing a streamer, or turning a streamer on or off, is checked in the same way
- Log tab - One message, `Priority: stopped <streamer> (lower priority)`, is written when a recording is stopped for its priority (see [Log tab](#log-tab))
- The order, the **Always record** switches and the limit are saved, so they carry on after jj-dlp restarts (see [Settings](#settings))

## Streamer settings

- Settings panel - Shown under the streamer's row when you click **Settings**. It is closed when you switch to the Livestreams tab. The fields are arranged in compact rows, the same size as the fields in the Site card (see [Settings tab](#settings-tab))
    - Edits are held as a draft until you click **Save changes**. Nothing is applied while you edit
																												 
																				   
																																													   
    - Display Name - The name shown for the streamer, in the first row of the panel
        - Leave blank to use the username
        - A change is saved with **Save changes**, and the new name is shown everywhere the streamer's name is shown
    - Subfolders - The subfolders this streamer's recordings are saved in (see [Recording subfolders](#recording-subfolders))
        - **Inherit** - Uses the layout of the site. The entry shows the site's current layout (example: `Inherit (site\streamer\)`)
        - The other entries are the same layouts as in the Site card, and apply to this streamer only
        - Effective path - Shown under the first row, with where this streamer's recordings will be saved, with the streamer's name (example: `Effective path: C:\Users\owner\Videos\jj-dlp\Twitch\Emiru\<filename>.mp4`). It follows the draft
        - A warning is shown under the path when the site's recorder command has an absolute `--output` path, which stops subfolders from applying
    - Notifications - Whether notifications are sent for this streamer (see [Notifications](#notifications))
        - **Inherit** - Uses the site's **Enable Notifications** switch. The entry shows the site's current setting (example: `Inherit (On)`)
        - **On** and **Off** - Apply to this streamer only. **On** sends notifications even when the site's switch is off
																																						 
        - The setting applies to went live, stalled, restarted, interrupt and error notifications. The channels, the cooldown and the **Notify** switches still come from the site
        - No notification is sent while all notifications are disabled globally, whatever this setting is (see [Notifications](#notifications))
    - Split recordings - Whether this streamer's recordings are split into parts (see [Split recordings](#split-recordings))
        - **Inherit**, **On** and **Off** - **Inherit** uses the site's settings, and the entry shows the site's current setting (example: `Inherit (On)`). **On** and **Off** apply to this streamer only
        - **Split the recording after (minutes)** box - How many minutes each part runs, shown beside the **Split recordings** list. Only digits can be typed, up to 6. With **Inherit** chosen, a blank box uses the site's length
        - Typing a length chooses **On**, and clearing the box chooses **Inherit**. Choosing **Inherit** or **Off** clears the box
        - The hint in the box shows the length of the site while **Inherit** is chosen, or nothing when splitting is off for the site. There is no hint while **On** or **Off** is chosen
        - The box has a red border while **On** is chosen and the box is blank, and while it holds a value below 1. **Save changes** is unavailable until a length is entered
																										 
																																							  
																													
														  
    - Intro delay - Holds back this streamer's recording after they go live (see [Intro delay](#intro-delay))
        - **Intro Delay** - Turns the delay on or off for this streamer
																																															
        - **Split the intro into its own file instead of waiting** - Records right away and splits the intro off after the minutes (see [Intro delay](#intro-delay)). It is greyed out, shown off and cannot be changed while **Intro Delay** is off. The choice is kept, and shown again when **Intro Delay** is turned on
        - **Delay starting the recording by (minutes)** box - How many minutes to wait after the streamer goes live. Only digits can be typed, up to 6. Typing a number turns **Intro Delay** on. The box has no hint text
        - The box has a red border while **Intro Delay** is on and the box is blank or 0. **Save changes** is unavailable until a number of at least 1 is entered. With **Intro Delay** off, the box can be left blank, and a blank box is saved as 0, which means no delay
    - Schedule - Switches this streamer on and off automatically (see [Schedule](#schedule))
        - **Schedule** - Turns the schedule on or off for this streamer
        - Mode list - **One-off** or **Recurring**, shown beside the **Schedule** switch
        - One-off - **Start** and **End** rows, each with a date button and a time spin box
        - Recurring - A row of day buttons, and a row with **From** and **To** time spin boxes
        - Time spin box - Three parts: hour (1-12), minute (00-59) and AM/PM. Each part has a **-** and a **+** button. The hour and minute wrap around without changing AM/PM, and either AM/PM button switches between AM and PM. A time that is not set yet shows `--`, and the first press starts from 12:00 AM
        - A date button is shown with a red border, and a time spin box with red text, while the schedule is turned on and its value is missing or not valid, and a message under the schedule says what is wrong. **Save changes** is unavailable until it is fixed
    - Name restarted recordings _partN - Whether a recording that starts again during the same stream gets a `_partN` suffix (see [Auto-suffix](#auto-suffix))
        - **Inherit** - Uses the site's switch. The entry shows the site's current setting (example: `Inherit (On)`)
        - **On** and **Off** - Apply to this streamer only
    - Upgrade quality when available - Whether this streamer's recording is restarted when the stream offers a higher quality (see [Upgrade quality](#upgrade-quality))
        - **Inherit** - Uses the site's switch. The entry shows the site's current setting (example: `Inherit (On)`)
        - **On** and **Off** - Apply to this streamer only
    - Quick check ended streams - Whether this streamer is checked again on its own shortly after its recording ends (see [Quick check](#quick-check))
        - **Inherit** - Uses the site's switch. The entry shows the site's current setting (example: `Inherit (On)`)
        - **On** and **Off** - Apply to this streamer only. The delay always comes from the site
    - Check every (seconds) - How often this streamer is checked, in seconds, from 5 up. The time is counted from when its previous check has finished
        - A blank box (the default) uses the site's **Check every (seconds)**, and the hint in the box shows the site's current value. Only digits can be typed, up to 6
        - A streamer with its own value is left out of the site's single yt-dlp call and is checked at that interval instead. Streamers on the same site with the same value are checked together in one yt-dlp call, and a streamer with a unique value gets a call of its own. It keeps being checked while it is recording
        - The box has a red border while it holds a value below 5. **Save changes** is unavailable until the box is blank or holds a number of at least 5
    - Pause checking - Stops this streamer from being checked. Off by default, and shown at the bottom left of the panel, on the same row as the **Save changes** button
        - A paused streamer is left out of the site's single yt-dlp call and of any same-interval call. No quick check is made for it either
        - A recording that is already running carries on. A streamer that is not recording and was shown live is shown offline
        - It is separate from **Record**, which stops recording but keeps checking, and from the site's **Pause checking**, which stops the checks of every streamer on the site (see [Settings tab](#settings-tab))
    - Save changes button - Saves your edits. Unavailable while there are no edits, or while the intro delay, split length, check interval or schedule is invalid
    - Revert button - Discards your edits. Unavailable while there are no edits
    - Defaults button - Fills the panel with the settings a streamer starts with, and does not change the display name. Nothing is saved until you press **Save changes**, and **Revert** brings back the saved values
    - unsaved changes - A chip shown beside the buttons while the draft is different from what is saved. **Always record** is set on the Priority page and is not part of the draft, so turning it on or off does not show the chip (see [Priority](#priority))
																																								  
- A subfolder change applies to the next recording. A recording that is already running keeps its folder
- An intro delay change never stops a recording that is already running (see [Intro delay](#intro-delay))
- A schedule change applies as soon as the streamer's settings are saved (see [Schedule](#schedule))
- A split change applies to a recording that is already running (see [Split recordings](#split-recordings)). An auto-suffix change applies the next time a recorder starts. An upgrade quality change applies from the next check (see [Upgrade quality](#upgrade-quality)). A quick check change applies from the next recording that ends, and turning it off cancels a quick check that is waiting (see [Quick check](#quick-check)). A check interval change applies as soon as it is saved, and the streamer is checked again right away. A **Pause checking** change applies as soon as it is saved, and turning it off checks the streamer again right away
																																										   

## Posts

- Posts tab - The **Posts** tab, to the right of the **Livestreams** tab. It shows the posts of the accounts you have added, with a thumbnail for each (see [Livestreams tab](#livestreams-tab))
    - Sidebar - Lists the library and the accounts. Click an entry to choose which posts are shown
        - Library - **Unviewed**, **Viewed** and **Starred**, each with the number of posts in it, and **Failed** while at least one download has failed. The numbers follow the selected account
        - Accounts - **All accounts** and each account, with the number of unviewed posts, or the number of viewed or starred posts while **Viewed** or **Starred** is selected. The accounts are grouped under the name of their site, with the time to that site's next check beside the name (example: `<site> • 59s`). The time is replaced by `checking…` while that site's check is running, and by `paused` while checking is paused for the site. A site with no accounts is not shown, and a site whose accounts are all turned off shows only its name. An account whose last check failed is marked with `⚠`, and an account that is paused is marked `(off)`
        - Remove button - Shown at the bottom of the sidebar while an account is selected, as `Remove @<username>`. Pressing it stops watching that account right away, without asking, and the sidebar goes back to **All accounts**. The posts that were already found are removed from the list too, along with their thumbnails. Downloaded files stay where they were saved
        - **Check all now** - Checks every site right away
        - **Pause all** and **Resume all** - Pause or resume checking on every site at once. The button shows **Resume all** while every site is paused. The setting is saved per site, so it matches the **Pause checking** switch of each site (see [Posts site settings](#posts-site-settings))
																																																		 
    - Toolbar - Shown above the posts
        - Title - The name of the selected list and the number of posts in it
        - Search - Shows only the posts whose caption or account contains the text
        - **Newest first** - Switches between the newest and the oldest posts first. It is saved
        - Size slider - Sets how wide the post cards are. It is saved when you release the slider
        - **Mark all viewed** - Shown on the **Unviewed** list while it has posts. Marks every post that is shown as viewed, after the account and the search have been applied
        - Status message - A short message is shown beside the title after an action, and goes away after a few seconds
    - Posts list - Post cards in rows that fill the width of the window, a page at a time
        - **Show more** - Shown at the end of the list, with how many posts remain. Changing the list, the account or the search starts again from the first page
        - Account errors - A warning line is shown above the posts for each account whose last check failed. It clears on the next check that succeeds. The Log tab gets one message when a problem appears or changes (see [Log tab](#log-tab))
    - Add bar - Shown at the bottom of the page
        - Site list - Chooses the site the account is added to. It lists every site template and every site you have set up on the Settings tab. A template that has no site yet gets its site, with the template's settings, when the first account is added to it
        - Username - The account's username, or its profile URL. A typed username is added in lowercase letters. The profile URL is built from the site's **Profile URL template** (see [Posts site settings](#posts-site-settings))
            - A profile URL that ends in `/videos` is named after the part before it
            - Video URL - For a site that supports it, the URL of any video the account posted can be entered instead. The account is named after the user in the URL, not after the video (see [Account ID lookup](#account-id-lookup))
        - **Add** button - Adds the account and starts checking it right away. Unavailable while the box is blank. Pressing Enter in the box does the same
        - Accounts can no longer be added on the Settings tab
        - Pulse - While no account has been added yet, the **Username** box pulses so it is easier to find. It stops pulsing once the box is clicked or an account has been added
    - Empty page
																																					
        - `No accounts yet. Pick a site and add a username below.` is shown when there is no account
    - Post card - The thumbnail, the state of the post, and buttons
        - Thumbnail - A preview of the post. Clicking it does the same as **Open**, except while the post is `Downloading…`, when clicking it does nothing. `No preview` is shown when no preview could be made. The state of the post is shown in the top left corner, and the length of the video in the bottom right corner
            - A picture icon is shown in the bottom left corner of the card of a photo, and no length is shown (see [TikTok photos scraper](#tiktok-photos-scraper))
        - Under the thumbnail - The account and how long ago the post was made, then the caption. `(no caption)` is shown when the post has none
            - When a site's list of posts does not include the time a post was made or the length of its video, the time the post was found is used and no length is shown, until the post has been downloaded. Then the real time and length are shown
        - Viewed posts, and posts that were opened and are waiting to be marked viewed, are shown dimmed
        - `Queued`, `Downloading…`, `Failed` and `Not saved` - The state of a post that has not been downloaded. The reason is shown under a `Failed` post
        - X button - Shown in the top right corner of the thumbnail of a post that is `Not saved` or `Failed`. Turns red when the mouse is over it. Removes the post from the library, and it is not found again by later checks. Downloaded posts do not have it
        - **Save** - Downloads a post that is `Not saved`. The limits for auto-downloads do not apply to it (see [Auto-download limits](#auto-download-limits))
        - **Retry** - Downloads a `Failed` post again, also without the limits for auto-downloads
        - **Cancel** - Shown in place of **Save** and **Retry** while a post is `Queued` or `Downloading…`. Stops the download, deletes the unfinished files of the post and puts the post back to `Not saved`, without an error
        - Error - Any error text shown on a post goes away 10 seconds after it appears, whatever caused it. A post that failed stays `Failed`, and **Retry** is still available
        - **Open** - Opens the downloaded post in your default app. A photo is opened in your default image viewer. When the post has not been downloaded, it plays the post's stream link while **Open m3u8 instead of site** is on for the site, and otherwise opens the post's page (see [Open m3u8 instead of site](#open-m3u8-instead-of-site))
        - **Viewed** and **Unview**, the star, **Page**, **Folder** and **Delete** - Mark the post, keep it, open its page, show it in your file manager, or send its file to the recycle bin
            - Starring a post adds it to the **Starred** list and keeps it from [Auto-delete](#auto-delete)
            - **Delete** keeps the post in the list and puts it back to `Not saved`, with its thumbnail kept. **Save** downloads it again
            - **Delete** also removes the folder the file was in when it is left empty. The download folder itself is never removed
            - **Delete** on a photo removes its card from the list instead, since a single photo of a post cannot be downloaded again. The other photos of the post are not affected
            - The status message names the file that was sent to the recycle bin. When the saved file cannot be found, the post's id is looked for in the download folder. When there is no file, the message says so, and the post is still put back to `Not saved`
    - Leaving the tab - Switching to another tab sets the list back to **Unviewed** and the account back to **All accounts**, so the tab is in that state the next time it is opened. The search and the sort are not changed
- Opening a post - A post that is opened is dimmed at once and stays in the **Unviewed** list, then is moved to the viewed posts after the site's **Mark viewed after (seconds)**
    - Only applies while **Mark a post viewed when it is opened** is on for the site
    - A post that is played from its stream link is dimmed once the player has started
    - The wait is kept when jj-dlp is closed, so a dimmed post is moved after the wait has passed. **Viewed** and **Unview** cancel the wait
- First check of an account - The posts found on an account's first check are shown as unviewed, whether or not they are downloaded
    - While **Download existing posts when an account is first checked** is off, they are listed as `Not saved` and are not downloaded. They can be downloaded with **Save**
    - No notification is sent for the first check of an account
- Background checking - The Posts checks and downloads are run by jj-dlp itself, not by the window, so they carry on whichever tab or page is showing
    - With `--no-gui` they run too, and their messages are printed to the terminal (see [Command-line modes](#command-line-modes))
    - A window that is opened later shows everything that was found in the meantime
    - Each site is checked on its own interval, independently of the other sites, so a check that takes a long time on one site does not delay the others. **Check all now** checks every site right away
        - The interval is counted from when the site's check has finished
        - A site's accounts are checked one after another, with one yt-dlp command for each account
- Twitter/X Helper - For a site whose **Twitter/X Helper** is not **None**, an X account's posts are found by the helper instead of by the checker command (see [Twitter/X helper](#twitterx-helper))
- TikTok Photos scraper - For a site with the **Use TikTok Photos scraper** switch turned on, a TikTok post that is a photo post is downloaded from its page instead of by yt-dlp (see [TikTok photos scraper](#tiktok-photos-scraper))
- Downloads - Posts are downloaded in the background, a couple at a time, while **Download new posts automatically** is on for the site
    - A download that fails is shown as `Failed`, and is tried again automatically after later checks, until it has failed a few times. **Retry** tries again at once and starts the count over
        - The unfinished files of the post, including a partly written video, are deleted from the download folder
    - A download that was interrupted when jj-dlp closed is queued again at the next launch
    - A download that is over the site's size limit or time limit is stopped, and the post stays `Not saved` (see [Auto-download limits](#auto-download-limits))
    - Finding the file - yt-dlp writes the path of the finished file to a small file in the `post_sidecars` folder inside the settings folder, so a file name can hold any character, such as an emoji (see [Settings](#settings))
        - The same small file holds the time the post was made and the length of the video, which are used when the list of posts did not include them
        - The small file is deleted after each download. If it is not there and yt-dlp finished without an error, the file is found by the post's id in the download folder
        - When yt-dlp ends with an error and has not reported a finished file, no file is looked for, and the post is `Failed`
    - The Log tab gets one message for each download that fails (see [Log tab](#log-tab))
- Thumbnails - Saved when a post is found, from the preview image the post has, at a smaller size
    - When a post's list entry has no preview image or no caption, jj-dlp asks yt-dlp for the post's details with one extra call, and one message is written to the Log tab for each post (see [Log tab](#log-tab))
    - When a post has no usable preview image, a frame of the video is saved with ffmpeg once it has been downloaded (see [Requirements](#requirements))
    - The thumbnail of a photo is made from the photo itself once it has been downloaded
    - A thumbnail is removed when its account is removed. Deleting a post's file keeps it, except for a photo, whose card is removed
- Notifications - One notification is sent for each account that has new posts after a check, with the number of new posts and the first caption (see [Settings tab](#settings-tab))
    - Needs the site's **Enable Notifications** switch, and at least one of **Desktop Notifications** and **Mobile Notifications (ntfy)**, set on the **Posts** sub-page of the Notifications page
    - None are sent while all notifications are disabled globally. A site with a blank ntfy topic uses the global topic (see [Notifications](#notifications))
    - Tapping an ntfy notification opens the first new post
    - Each notification ends with the free space left on the drive of the site's download folder (see [Disk space](#disk-space))
    - The first check of an account sends none
    - The Log tab gets one message when new posts are found on an account (see [Log tab](#log-tab))
												

### Posts site settings

- Posts site settings - Shown on the **Posts** page of the Settings tab, with the card of the site chosen in the site list (see [Settings tab](#settings-tab))
    - Site list - Shown at the top of the page, in the same way as on the **Livestreams** page. Chooses the site whose card is shown. It lists every site you have set up and every site template that has no site yet, followed by **New site** (see [Settings tab](#settings-tab))
        - Choosing a template that has no site yet adds the site and shows its card. Choosing a template for a site that already exists does not add a second site. The card of the existing site is shown instead
        - **New site** opens an empty card that is not saved until you press **Add site**. **Discard** closes it
																																												  
        - A template for a site's recorded streams (VODs) is offered with the others. It fills in a profile URL template that points to the account's recorded streams, and uses their stream link instead of the post's page (see [Open m3u8 instead of site](#open-m3u8-instead-of-site))
																																											  
        - A template for a site that needs your login adds `--cookies-from-browser firefox` to its **Checker arguments** and **Downloader arguments**, so Firefox must have a profile (see [Requirements](#requirements))
        - The **Twitter/X** template uses the **gallery-dl** helper, checks every 900 seconds, and has `--cookies-from-browser firefox` in its checker and downloader arguments (see [Twitter/X helper](#twitterx-helper))
        - The **Youtube Videos** template, shown below **Youtube Shorts**, uses the profile URL template `https://www.youtube.com/@{username}`, checks only the newest post (`--playlist-end 1`), and starts with a size limit of 1000 MB and a time limit of 120 seconds for auto-downloads
        - A template does not need to be chosen before accounts can be added. Its site is created when the first account is added to it from the Posts tab, in the same way as on the **Livestreams** page (see [Posts](#posts))
        - Posts settings are no longer shown in a General section. Every one of them is set per site
    - Site card
        - Site name (example) - The display name for the site. Required. The field has a red border while it is blank
        - Check every (seconds) - How often the site's accounts are checked, from 15 up, counted from when the site's check has finished. A blank field uses the fallback shown as hint text. Save changes is unavailable until it is blank or a valid number. The Twitter/X template starts at 900
        - Path to yt-dlp executable - The full path to yt-dlp. Leave blank to use the fallback shown as hint text
        - Profile URL template - The profile address for the site, with `{username}` where the account's name goes. Required, and it has no hint text. The field has a red border while it is blank, and Save changes and Add site are unavailable until it has a value
        - Download folder (blank = global/posts) - Where the site's posts are saved. **Browse** chooses a folder and **Open** opens it
        - Delete files of unstarred posts after (days, blank = never) - Turns on [Auto-delete](#auto-delete). Save changes is unavailable until it is blank or a whole number of at least 1
        - Twitter/X Helper: - A dropdown with **None**, **Basic HTML scraper** and **gallery-dl**. It chooses what finds the posts of X accounts on the site, and the default is **None**, which uses the checker command like any other site (see [Twitter/X helper](#twitterx-helper))
        - Path to gallery-dl executable - The full path to gallery-dl. Shown to the right of **Twitter/X Helper:** only while **gallery-dl** is chosen. Leave blank to use the fallback shown as hint text (see [gallery-dl path](#gallery-dl-path))
        - gallery-dl arguments - The gallery-dl arguments or options to pass when listing an account. Shown only while **gallery-dl** is chosen. Leave blank to use the fallback shown as hint text. `{url}` is replaced by the account's profile URL followed by `/tweets`
        - Download X photos - Saves the pictures of X photo posts, with one card for each picture. Shown only while **gallery-dl** is chosen, at the top of the switches. It is on by default (see [Twitter/X helper](#twitterx-helper))
        - Notifications - The notification settings of the site are not on the card. They are set on the **Posts** sub-page of the Notifications page (see [Settings tab](#settings-tab))
																																												  
																																											
																																																																																			
        - Pause checking - Stops the site from being checked. Shown at the bottom left of the card, on the same row as the **Save changes** button
        - Use TikTok Photos scraper - Saves the pictures of TikTok photo posts from the post's page, with one card for each picture. The TikTok template has it on. (see [TikTok photos scraper](#tiktok-photos-scraper))
        - Download new posts automatically - Downloads new posts as soon as they are found
        - Download existing posts when an account is first checked - Also downloads the posts an account already has when it is first checked
        - Mark a post viewed when it is opened - Dims a post when it is opened and moves it to the viewed posts later (see [Posts](#posts))
        - Mark viewed after (seconds) - How long an opened post stays in the unviewed posts before it is moved. Shown to the right of **Delete files of unstarred posts after**. Default 30. Save changes is unavailable until it is blank or a whole number of at least 0; blank uses 30, and 0 moves the post at once
        - Open m3u8 instead of site - Plays a post that is not downloaded from its stream link, instead of opening its page (see [Open m3u8 instead of site](#open-m3u8-instead-of-site))
        - Size limit for auto-downloads (MB, blank = none) - Skips an automatic download that is larger than this many megabytes. Shown to the left of **Time limit for auto-downloads**. Save changes is unavailable until it is blank or a whole number of at least 1 (see [Auto-download limits](#auto-download-limits))
        - Time limit for auto-downloads (seconds, blank = none) - Cancels an automatic download that takes longer than this many seconds. Shown to the right of **Size limit for auto-downloads**. Save changes is unavailable until it is blank or a whole number of at least 1
        - The two limits are shown in one row with **m3u8 arguments** and **Player for m3u8**, and the four fields are the same width
        - m3u8 arguments - The yt-dlp arguments or options to pass when getting the stream link of a post. Leave blank to use the fallback shown as hint text. `{url}` is replaced by the post's URL, and the first line that is printed as a web address is used as the stream link
        - Player for m3u8 - The program that plays the stream link, as a full path or a name on your `PATH`. Leave blank to open the link with your system's default app
        - Checker arguments - The yt-dlp arguments or options to pass when checking an account for new posts. Leave blank to use the fallback shown as hint text
            - With a Twitter/X Helper chosen, the arguments are used on the address of each X post that the helper finds (see [Twitter/X helper](#twitterx-helper))
        - Downloader arguments - The yt-dlp arguments or options to pass when downloading a post. Leave blank to use the fallback shown as hint text
            - jj-dlp adds its own `--print-to-file` options to the end of the command, so it can find the file that was saved and read the time and length of the post (see [Posts](#posts))
            - A TikTok photo post is not downloaded with these arguments while the **Use TikTok Photos scraper** switch is on (see [TikTok photos scraper](#tiktok-photos-scraper))
            - For an automatic download that has a size limit, jj-dlp also adds a `--max-filesize` option
        - The **SITE** heading is no longer shown, and the explanation lines on the page are gone
    - Buttons - Shown at the bottom of each card, in the same way as on the **Livestreams** page
        - Save changes button - Saves your edits. Unavailable while there are no edits, or while the site name or profile URL template is blank or any field is invalid
        - Revert button - Discards your edits. Unavailable while there are no edits
        - Defaults button - Fills the card with the settings of the site template with the same name as the site, or the **New Site** settings for any other name. The site name is kept. Nothing is saved until you press **Save changes**, and **Revert** brings back the saved values
																																										 
        - unsaved changes - A chip shown beside the buttons while the card is different from what is saved
        - Edits are held as a draft until you click **Save changes**. Nothing is applied while you edit

### Auto-delete

- Auto-delete - Deletes the files of unstarred posts a set number of days after they were found, so the files do not take up space for ever
    - Set per site with **Delete files of unstarred posts after (days, blank = never)**. A blank field never deletes files. Every template except Twitch Vods starts at 30 days, including New Site
    - Starred posts are never deleted, and only posts that have a downloaded file are affected, so a post that is `Not saved`, `Queued`, `Downloading…` or `Failed` is left as it is
    - The age is counted from when the post was found, not from when it was posted, so older posts that are found on an account's first check are not deleted at once
    - The file is sent to the recycle bin, in the same way as the **Delete** button of the post card. The post stays in the list as `Not saved`, with its thumbnail, and **Save** downloads it again. The folder it was in is removed when it is left empty
    - Checked about once an hour, and right after the site's settings are saved
- Log tab - One message is written when files have been deleted (see [Log tab](#log-tab))

### Auto-download limits

- Auto-download limits - Stop an automatic download that is too large or takes too long, so a big post does not fill the disk or keep a download running
    - Set per site with **Size limit for auto-downloads (MB, blank = none)** and **Time limit for auto-downloads (seconds, blank = none)** (see [Posts site settings](#posts-site-settings))
    - A blank field turns that limit off, and the hint text shows `None`. The two limits work separately
    - Only apply to automatic downloads, which are the downloads made by **Download new posts automatically** and the automatic tries of a `Failed` post (see [Posts](#posts))
    - **Save** and **Retry** are never held back by the limits, so a post that was skipped can be downloaded by hand
- Size limit - jj-dlp gives yt-dlp a `--max-filesize` option, and yt-dlp stops the download when the file is larger than that many megabytes
    - It only works when yt-dlp knows the size before it downloads. A post whose size is not known is not stopped by it, and the time limit still applies
- Time limit - The download is cancelled when yt-dlp has run for longer than that many seconds
    - The time counts the whole of the yt-dlp run, which includes finding the video and joining its parts, not only the transfer
    - The unfinished files of the post, including a partly written video, are deleted from the download folder
																						
- A skipped post - Stays in the list as `Not saved`, is not shown as `Failed` and is not tried again by later checks
    - **Save** downloads it, without the limits
- Log tab - One message is written for each post that is skipped, with the reason (see [Log tab](#log-tab))

### Twitter/X helper

- Twitter/X Helper - Chooses what finds the newest posts of an X account, per site, with the **Twitter/X Helper:** dropdown on the site's card (see [Posts site settings](#posts-site-settings))
    - **None** - The site's **Checker arguments** list the profile with yt-dlp, like any other site. This is the default
    - **Basic HTML scraper** - Reads the profile page as a visitor who is not logged in sees it, using the user agent from the site's arguments when one is set
    - **gallery-dl** - Runs gallery-dl on the profile with your Firefox cookies, so it can see accounts and posts that need a login
    - jj-dlp never switches from one helper to another by itself, so a gallery-dl problem is shown as an account error and the basic scraper is not tried
    - The Twitter/X template uses **gallery-dl**. A Twitter/X site that existed before this setting was added is switched to **gallery-dl** once, and its **Check every** is set to 900 seconds (see [Settings resets](#settings-resets))
- Finding posts - The newest posts are looked at, newest first, and only the posts of the account are kept
    - Posts of other accounts, such as reposts, are left out
    - With **Basic HTML scraper**, the posts are the ones linked on the profile page. A pinned post is looked at like any other post, so an older pinned post can be found on the first check
    - With **gallery-dl**, the posts are the first files of the profile's media, which are listed with `--range 1-5` in the default arguments, so about the newest five files are looked at. A post that is only text is never listed
    - A post that is not known yet is checked once, and a post that was skipped is not checked again until jj-dlp restarts
- Videos - Each post with a video that is not known yet is checked with yt-dlp, using the site's **Checker arguments** on the post's own address
    - A post that has a video is added to the list. A post that only has pictures is skipped, unless the **gallery-dl** helper is on and **Download X photos** is on
    - A post that fails for another reason, such as a network problem, is checked again at the next check
    - A post with more than one video is one post, and uses the first video
    - The post's page is the post's link
- Photos - With **gallery-dl** and **Download X photos** on, a post that has pictures is added to the list from gallery-dl's listing, without a yt-dlp check
    - The caption is the post's text, or `Photo post` when it has none, and the time is the time of the post
    - The thumbnail is a small version of the first picture, and the links to the full-size pictures are kept with the post, so downloading it does not run gallery-dl again
    - Cards, files, limits and failures work as for TikTok photo posts, and the pictures are named `account_date_postid_N.ext` (see [TikTok photos scraper](#tiktok-photos-scraper))
    - Needs no login to download. The pictures are loaded from X's image servers as a visitor sees them
    - A photo post that was skipped before the switch was turned on is looked at again after jj-dlp restarts
- gallery-dl run - One gallery-dl command is run for each account at each check, using **gallery-dl arguments**, and it is given the profile address followed by `/tweets`
    - The default arguments are `--cookies-from-browser firefox -j --range 1-5 {url}`, and `{url}` is the profile address with `/tweets` added, so the command lists the account's own posts. jj-dlp reads the JSON that `-j` prints, so keep `-j` in the arguments
    - The command is stopped after 180 seconds. A rate limit from X can make gallery-dl wait, so it can run into that limit
    - The stdout and stderr logs of the account's checker also hold gallery-dl's output (see [Posts](#posts))
- Problems - Shown as an account error, in the same way as other check problems (see [Posts](#posts))
    - The profile page cannot be loaded (**Basic HTML scraper**)
    - gallery-dl cannot be started, takes too long, or prints an error such as a missing login. The last `[error]` line of its output is shown
    - No posts are found, which can mean the account is private, has no pictures or videos, or Firefox is not logged in to X
    - Every post that was checked failed for a reason other than having no video
    - The Log tab gets one message when a problem appears or changes (see [Log tab](#log-tab))

### TikTok photos scraper

- TikTok photos scraper - Saves the pictures of a TikTok photo post, which yt-dlp does not download (yt-dlp only saves the post's sound)
    - Turned on per site with **Use TikTok Photos scraper** on the site's card (see [Posts site settings](#posts-site-settings))
    - Applies to posts whose address is on tiktok.com. Every other post is downloaded as usual
    - Needs no login. The post's page is loaded as a visitor who is not logged in sees it, using the user agent from the site's arguments when one is set
- Finding photo posts - When a TikTok post is downloaded, its page is read first
    - A post that has pictures is saved by jj-dlp. A post that is a video is downloaded by yt-dlp as usual
    - When the page cannot be read, for example because TikTok asks for a check, the post is downloaded by yt-dlp as usual, which saves its sound only. The reason is written to the site's downloader log while that log is on (see [Posts site settings](#posts-site-settings))
- Cards - A photo post with several pictures gets one card for each picture
    - The first picture keeps the post's card and the others are added when the download finishes. The caption of each card ends with its place, such as `(2/5)`
    - Each card has the picture icon in its bottom left corner, shows the picture as its thumbnail, and has its own **Open**, **Viewed**, star, **Folder** and **Delete** buttons
    - A post that is not downloaded yet looks like any other post, because its page is only read when it is downloaded
- Files - The pictures are saved in the site's download folder next to the other posts, named `account_date_postid_N.ext`, where `N` is the picture's place in the post
    - The pictures are saved as TikTok sent them, as JPEG, PNG or WebP files, without being changed
    - **Open** opens the picture in your default image viewer
- Limits - The limits for auto-downloads count the pictures of a post together (see [Auto-download limits](#auto-download-limits))
    - A post over the size limit or the time limit stays `Not saved`, and **Save** downloads it without the limits
    - A download that is not limited, such as **Save**, stops at 500 MB for all the pictures of a post
- Failures - A post whose pictures cannot all be downloaded is `Failed`, and no pictures of it are kept. It is tried again in the same way as other failed downloads (see [Posts](#posts))
- Not covered - The sound of a photo post is not saved, and posts that were downloaded before the switch was turned on are not downloaded again

### TikTok Account ID lookup

- TikTok Account ID lookup - Adds an account from the URL of one of its videos, and keeps the account's ID
    - Used when the **Username** box of the add bar holds a video URL of a tiktok post (see [Posts](#posts))
    - Needs no setting
- Adding - The ID is looked up with yt-dlp from the video before the account is added
    - A status message is shown while the lookup works
    - When the lookup works, the account is added with its ID stored, and starts being checked right away
    - When the account already exists, the ID is stored on it, it is turned on again and its error is cleared, and no second account is added
    - When the lookup fails, nothing is added, the status message says `Account ID resolution failed. Check that the video URL is public and try again.` and the reason is written to the Log tab (see [Log tab](#log-tab))
    - The lookup uses the user agent from the site's **Checker arguments** when one is set, and none otherwise
- Checking - A check that fails because the account cannot be found from its profile address is handled in one of two ways
    - With an ID stored, the check is run again with the ID, and the account is checked as normal
    - Without one, the account shows an error that asks for the URL of any video from the account. Enter that URL in the add bar to fix it. The account is kept, and its posts are not removed
- Automatic ID - Every successful check stores the ID found in the results, so a later failure is handled without any action
- The ID is saved with the account in `posts.json` (see [Settings](#settings))

### Open m3u8 instead of site

- Open m3u8 instead of site - Plays a post that is not downloaded from its stream link, so a long video can be watched without downloading it first
    - Set per site with **Open m3u8 instead of site** (see [Posts site settings](#posts-site-settings))
    - Only changes **Open** on a post that is not downloaded. A downloaded post still opens its file, and the **Page** button still opens the post's page
- Getting the link - Pressing **Open** runs the site's yt-dlp with the **m3u8 arguments** on the post's URL, and the status line shows `Getting the stream link…` while it works
    - The link is not saved, so it is found again each time **Open** is pressed
- Playing - The link opens in the program set in **Player for m3u8**
    - With that field blank, jj-dlp saves a small `.m3u8` file in your system's temporary folder and opens it with the system's default app for that file type. If that app is not a media player, set **Player for m3u8**
    - The post is dimmed once the player has started, then moved to the viewed posts after **Mark viewed after (seconds)**, following **Mark a post viewed when it is opened**
- Problems - A message is shown in the status line, and written to the Log tab, when the link cannot be found or the player cannot be started (see [Log tab](#log-tab))

## Processes tab

- Header - Shows how many are recording
    - A streamer has two recorder cards for a short time while a split starts the next part (see [Split recordings](#split-recordings))
- Recorder card - One per running recorder, with the site shown as a rounded tag in its site color (see [Site color](#site-color) and [Process panel](#process-panel-1))
    - State - A red `STALLED N×` chip is shown next to the state when the recording file is missing or has stopped growing, and the status dot turns red too (see [Recording file checks](#recording-file-checks))
        - Clicking the `STALLED N×` chip clears it for that streamer, in the same way as on the Livestreams tab (see [Livestreams tab](#livestreams-tab))
    - Open button - Opens the file the recorder is writing, in your default app. Disabled until the file is verified and exists
    - Expanded card - Shows the recorder's full output in an **Output** panel while **Enable command output parsing** is on, above the **Last line** panel (see [Process panel](#process-panel-1))
- Leaving the tab - Every expanded card is collapsed when you switch to another tab, so the output of the recorders is not drawn while the tab is not shown

## Files tab

- Header - The **FILES** title, followed by the status line, with switches on the right
														
    - Delete empty folders - Also deletes the subfolder a file was in when a delete or a move leaves it empty. The output folder itself, and any folder outside it, is never deleted
    - Hide posts - Hides the files of downloaded posts, so they are not listed when a posts folder is also used for recordings. Recordings in the same folder stay in the list
        - A file that is still being downloaded for a post is hidden as well
        - It applies within about a second of turning it on or off
    - The switches are saved
- Buttons - Shown in a row under the title
    - **Expand all** and **Collapse all** - Open or close every group at once. **Expand all** is only available while a group is collapsed, **Collapse all** is only available while a group is open, and both need **Group by** to be something other than **None**
    - **Group by** - A list shown to the right of **Collapse all**. Chooses how the files are grouped, with groups that collapse and expand (see below). It is saved
    - **Open** - Opens the selected file in your default app
    - **Show in folder** - Shows the selected file or folder in your file manager. On Windows and macOS the item is selected, and on Linux the folder that holds it is opened. A streamer or site group is not a folder on disk, so the status line says so and nothing is opened
    - **Fixup**, **Trim** and **Move** - Open a panel for the selected file (see below). The button of the open panel is highlighted, and pressing it again closes the panel, the same as **Cancel**. Pressing another one of the three switches to that panel
    - **Delete** - Sends the selected file to the recycle bin
    - **Open**, **Fixup**, **Trim**, **Move** and **Delete** need a file to be selected
- Status line - Shown on the title row, beside the **FILES** title. It shows the result of the last action, and goes blank after a few seconds. A message never moves the table, and no blank line is left between the buttons and the table
    - Running jobs are shown after the message, each with a **Cancel** button (see [Jobs](#jobs))
    - An open panel is shown between the buttons and the table
- Table - Lists every file in the output folders, including the files in their subfolders
    - Output folders - The output folder of every site, which is the site's own folder or the global one when it has none (see [Settings tab](#settings-tab)). A folder used by more than one site is listed once
        - With more than one output folder, each one has a header row with the site name and the path, and the number of files in it. Click a header row to collapse or expand it
        - With no sites, a message asks you to add a streamer on the Livestreams tab
        - A folder with no files shows `No files yet`
    - Columns - **Name**, **Status**, **Modified**, **Size** and **Rate**
        - Name - The header starts at the left edge of the table, and its label starts at the left edge of the header, not at the indented names below it
        - Status - `WRITING` while the file's size is still changing (a collapsed folder row also shows `WRITING` while any file inside it is writing), and `IDLE` once it has stayed the same for 3 seconds. While a job is running on the file, the job is shown instead: `Fixing…`, `Trimming…` or `Moving…`
        - Modified - The time the file was last changed, in your local time
        - Rate - How fast the file is growing, shown only while it is `WRITING`
    - Sorting - Click a column header to sort by it. The first click sorts ascending, and clicking the same header again flips the direction. An arrow shows the column and the direction
        - **Name** puts numbers in order by value, and ignores upper and lower case, so `part2` comes before `part10`
        - **Status** puts `IDLE` files first when ascending
        - **Modified** compares the time to the minute, and files in the same minute are put in name order
        - **Rate** sorts by the rate that is shown, so files that show `—` count as zero and are put together, in name order, ahead of the files that are writing when ascending. The fastest growing files come last when ascending
        - The column and the direction are saved
    - Group by - Chooses how the rows are grouped
        - **None** - One list, with the subfolder path shown in front of the file name
        - **Folder** - Each subfolder is a row, and the files of a subfolder are shown under it, indented
        - **Streamer** - Each streamer is a row, with the files of that streamer under it
        - **Site** - Each site is a row, with the files of that site under it
        - **Site\Streamer** - Each site is a row, with a row for each of its streamers under it, and the files under those
        - **Streamer\Site** - Each streamer is a row, with a row for each of its sites under it, and the files under those
        - With any grouping other than **None**, the subfolder path is not shown in front of the file name
    - Groups - Each group is a row with a ► when it is collapsed and a ▼ when it is open, and the number of files inside it. The files of a group are shown under it, indented
        - A collapsed group row shows `WRITING` in the Status column while any file inside it is `WRITING`, so a file that is being written is not hidden by the group. It is not shown while the group is open, because the file's own row shows it
        - A single click on a group row selects it and collapses or expands it
        - Groups are ordered by the sort, using the first file in each group. When sorted by **Name**, the streamer and site groups are in alphabetical order of their names instead, and descending reverses it
        - A group inside a collapsed group is not shown
        - Which groups are collapsed is not saved, so every group is open again after jj-dlp restarts
        - Other - Files whose streamer or site is not known are shown in a group named **Other**, which is always the last group. With **Site\Streamer** and **Streamer\Site**, a file needs both to be known, and is shown in **Other** when one is missing
    - Streamer and site of a file - Streamer and site groups need to know which streamer and site each file came from. They do not depend on the subfolders the files are saved in
        - Recorded files - When a recording's file is confirmed, jj-dlp remembers the streamer and the site it was recorded for, so the file is grouped under them even when it is renamed by a recording suffix, or saved with no subfolder (see [Recording file checks](#recording-file-checks))
        - Fixup, Trim and Move give the new file the streamer and site of the original, and so does the `_part1` rename of a recording's first file (see [Jobs](#jobs) and [Split recordings](#split-recordings))
        - A file that is renamed or moved outside of jj-dlp is no longer remembered, and is grouped in the same way as a file that was saved before jj-dlp remembered them
        - Files that are not remembered - The streamer is found from the file's path under the output folder, and then from its file name. Only a streamer that is in the Livestreams tab now is matched, and any other name leaves the file in **Other**
            - A subfolder with the same name as a streamer is checked first, in any upper or lower case
            - The file name must contain the whole streamer name, in any upper or lower case, so a name inside a longer word is not matched. When several streamers match, the one with the longest name is used
            - The site is the site of the matched streamer. When streamers with the same name are on more than one site, the file is grouped under the streamer, and is shown in **Other** when it is grouped by site
        - A remembered streamer keeps the name it had when the file was recorded, so renaming or removing the streamer does not move the file
        - What is remembered is kept in `file_owners.json` in the settings folder, and a file that is gone from a folder that still exists is dropped from it when jj-dlp starts (see [Settings](#settings))
    - Refresh - The list is refreshed every second while the tab is open and something is recording, being written or being moved, and less often while nothing is
        - The selection stays on the same item. When the selected file is gone, the selection moves to the next one in the list
        - With **Group by** set to anything other than **None**, the selection moves to the next file in the same group (folder, streamer or site), or to the previous file when it was the last one in the group. It only leaves the group when no file is left in it
        - After a file is deleted, or is gone because a move finished, the list scrolls only when the new selection would be outside the visible part of the list, and puts it in the middle. It stops at the top or bottom of the list when there are not enough rows to centre it. A selection that is already visible does not move the list
    - Moving - While a move is writing its new file, a `MOVING` row shows the new file's name, `WRITING`, its size and its rate. It cannot be selected
    - Footer - The number of files, their total size, and a reminder of the keys, including the keys that move the selection
- Selecting - Click a row to select it, and double-click a file to open it
    - **↑**, **↓**, **PgUp**, **PgDn**, **Home** and **End** move the selection, and the list scrolls to keep it in view
    - **Enter** opens the selected file, or collapses or expands the selected folder
    - **Space** shows the selected item in your file manager, the same as **Show in folder**
    - **Del** sends the selected file to the recycle bin
    - **Shift+Del** deletes the selected file permanently, without asking
    - **Esc** closes the open panel
    - The keys only work while no panel is open, so typing in a panel never deletes a file
- Delete - **Del**, **Shift+Del** and the **Delete** button
    - The result is shown in the status line, and a failure shows the reason
    - A file that a job is running on cannot be deleted. Cancel the job first (see [Jobs](#jobs))
    - With **Delete empty folders** on, the subfolder is deleted too when it is left empty, in the same way as the file: to the recycle bin after **Del**, and permanently after **Shift+Del**
- Fixup panel - Rewrites the selected recording with ffmpeg, without re-encoding, which repairs a recording whose file has problems
    - **Delete original when finished** - Sends the original file to the recycle bin once the new file is complete. When the new file has the same format, it takes the original's name
    - **Convert to MP4 (no re-encode)** - Saves the new file as an MP4
    - **Run fixup** starts the job. **Cancel** closes the panel
    - Without **Delete original when finished**, the new file is saved next to the original with `_fixed` added to its name. When converting to MP4 the new file has the `.mp4` extension instead. A name that is already used gets ` (2)`, ` (3)` and so on
    - The new file gets the modified time of the original
- Trim panel - Cuts a section out of the selected recording with ffmpeg, without re-encoding
    - **Start** and **End** - Each is three boxes for hours, minutes and seconds, with the `:` already in place, so nothing but digits is typed. A box holds up to 2 digits, and the cursor moves to the next box after the second digit. A blank box counts as 0
    - Minutes and seconds must be 59 or less, and **End** must be later than **Start**. A message in the status line says what is wrong, and the panel stays open
    - **Delete original when finished** and **Convert to MP4 (no re-encode)** - The same as in the Fixup panel
    - **Run trim** starts the job. **Cancel** closes the panel
    - The new file is saved next to the original with `_trimmed` added to its name, and a name that is already used gets ` (2)`, ` (3)` and so on
- Move panel - Moves the selected recording to any folder you choose
    - **Fixup the file during the move** - Rewrites the file with ffmpeg while moving it, in the same way as the Fixup panel
    - **Choose destination…** - Opens your system's save dialog, with the name of the file filled in. The dialog starts in the folder used for the last move. Choose a folder, and change the name if you like, then confirm to start the move right away. Closing the dialog without confirming does nothing
    - A file with the same name in the destination is replaced when you confirm the dialog's question about replacing it
    - The folder is saved, and the dialog starts there the next time
    - The destination folder is created if it does not exist
    - While the move runs, the new file is written under a temporary name, `<name>.moving.<extension>`, and renamed when it is complete. The original is deleted permanently once the new file is complete
    - Without the fixup, a move to the same drive just renames the file, which is instant. A move to another drive copies the file in pieces, and then deletes the original
    - With **Delete empty folders** on, the folder the file was in is sent to the recycle bin when the move leaves it empty
- Jobs - Fixup, Trim and Move each run in the background, so the tab and the rest of jj-dlp stay usable, even when you switch to another tab
    - Only one of each kind can run at a time. Starting a second one shows `A Fixup is already running - please wait`, and a file can only have one job at a time
    - **Cancel** - Shown on the status line for each running job. It stops ffmpeg, or the copy, and deletes the partly written file. The original is never changed by a job that is cancelled
        - Once the new file is complete and the job is deleting the original or renaming the new file, the button shows `Finishing…` and cannot be used
    - A job that fails shows the last error line of ffmpeg, and removes the partly written file
    - When the new file is complete but the original cannot be deleted, the original and the new file are both kept, and the message says so
    - A job that finishes, fails or is cancelled is shown in the status line, and written to the Log tab (see [Log tab](#log-tab))
- ffmpeg - Fixup, Trim and Move with the fixup option run ffmpeg from your `PATH`
    - If ffmpeg is not found, the status line shows `ffmpeg not found in PATH`
    - Unlike recording, the Files tab does not use `--ffmpeg-location` from a site's command, or an ffmpeg in the yt-dlp folder (see [Setup notices](#setup-notices))
    - Moving a file without the fixup does not need ffmpeg
- Settings - The sort column and direction, **Group by**, **Delete empty folders**, **Hide posts** and the folder of the last move are saved in `gui.json` (see [Settings](#settings))

## Settings tab

- Page switch - **Livestreams**, **Posts**, **Priority**, **Notifications** and **General** buttons at the top of the tab, with the **Open settings folder** button at the far right of the same row, shown on every page (see [Settings](#settings)).

- Posts page - Holds the settings of the Posts sites (see [Posts site settings](#posts-site-settings))

- Priority page - Holds the recording limit and the priority list (see [Priority](#priority))
    - Recording limit card - The **Recording limit** box, with the **Revert** and **Save changes** buttons on the right. A blank box means no limit, and only digits can be typed. It stays in place while the priority list scrolls, and shows an unsaved changes message beside the buttons while there are edits (see [Priority](#priority))
    - Priority card - The **PRIORITY** title with the **Move up** and **Move down** buttons, then one row for each streamer with its **Always record** switch. The title row stays in place, and only the rows scroll
    - Edits are held as a draft until you click **Save changes**. Nothing is applied while you edit

- Notifications page - Holds the notification and warning settings of the sites and the global notification settings (see [Notifications](#notifications), [Output warnings](#output-warnings), [Restart warnings](#restart-warnings) and [Recording file checks](#recording-file-checks))
    - Sub-pages - **Livestreams**, **Posts** and **General** buttons at the top of the page
    - Livestreams sub-page
        - Site list - Shown at the top of the sub-page. Chooses the site whose settings are shown, and every field below belongs to that site. It lists the sites you have set up on the Livestreams page
        - A note is shown under the site list while notifications or warnings are disabled globally (see below)
        - Edits are held as a draft until you click **Save changes**. The draft is the same one as in the Site card, so an edit made in either place is shown in both, and **Save changes** in either place saves both
        - Enable Notifications for <site> Livestreams - The master switch for notifications about the site's streamers (see [Notifications](#notifications)). A streamer's own setting can override it
        - Enable Warnings for <site> - Shown to the right of **Enable Notifications for <site> Livestreams**. Turns every warning chip of the site on or off at once. Greyed out and shown off while all warnings are disabled globally. The **Warn** choices of the list below are kept, and apply again when it is turned on
        - Desktop Notifications - Shows a desktop notification. Greyed out and shown off while **Enable Notifications** is off. The choice is kept, and shown again when **Enable Notifications** is turned on
        - Mobile Notifications (ntfy) - Sends an ntfy notification to the topic below. Greyed out and shown off while **Enable Notifications** is off, in the same way as **Desktop Notifications**
            - Save changes is unavailable while it is on and neither **ntfy topic** nor the global ntfy topic is valid
        - Send test button - Sends a test notification to the channels that are turned on, using the values on the page. Unavailable while neither channel is on, or while **Mobile Notifications (ntfy)** is on and no valid topic is set
        - Notification Cooldown (minutes) - The minutes before the same kind of notification is sent again for the same streamer, from 0 up. 0 turns the cooldown off. Save changes is unavailable until it is blank or a whole number of at least 0
        - Notification Timeout (seconds) - How many seconds the desktop notification is shown, from 1 up. Save changes is unavailable until it is blank or a whole number of at least 1
        - ntfy topic (example: j73hd92xl) - The topic name used for ntfy notifications. The field is greyed out, has no hint text and cannot be edited while **Mobile Notifications (ntfy)** or **Enable Notifications** is off. The field has a red border while both are on and no valid topic is set. A topic can use letters, digits, `-` and `_`, up to 64 characters
            - While the field can be edited and is blank, its hint text shows the global ntfy topic, which the site then uses
            - **Randomize** button - Shown beside the field. Fills it with a new random topic. Greyed out while the field cannot be edited. Nothing is saved until you press **Save changes**
            - Anyone who knows the topic name can read the notifications sent to it, so choose a name that is hard to guess. It is saved as plain text in `sites.json`
        - Wait for confirmed growth - Holds the went live notification until the recording file is seen growing
        - Notify when already live at launch - Also sends the went live notification when a streamer is already live at their first check after jj-dlp starts
        - Notifications and warnings list - Shown under the **NOTIFICATIONS AND WARNINGS** title, with **Notify**, **Warn** and **Flash** columns and a row for each of **Live**, **Stalled**, **Restarts**, **Interrupts** and **Errors**
            - **Live** - The first row. It has only a **Notify** switch, which turns the went live notification on or off for the site
            - **Notify** - Sends a notification for that kind (see [Notifications](#notifications)). Greyed out and shown off while **Enable Notifications** is off, while **Desktop Notifications** and **Mobile Notifications (ntfy)** are both off, or while all notifications are disabled globally
            - **Warn** - Shows the chip for that kind: `STALLED N×`, `RESTARTED N×`, `INTERRUPT N×` or `ERROR N×`. Greyed out and shown off while **Enable Warnings** is off, or while all warnings are disabled globally
            - **Flash** - Pulses the **Livestreams** tab button and colors a streamer's row of the site while the chip of that kind is showing (see [Livestreams tab](#livestreams-tab)). The **Live** row has none. Greyed out and shown off while the **Warn** switch of its row is off, while **Enable Warnings** is off, or while all warnings are disabled globally. The choice is kept, and applies again when they are turned on
                - While it is off for a kind, the tab button and the rows do not change color for that kind, and the chip and the **Clear warnings** button work as normal
            - Every new site, whether from a template or blank, starts with **Warn** and **Flash** on in the **Stalled** row. Existing sites keep their saved choices
            - The **Interrupts** and **Errors** switches are also greyed out and shown off while **Enable command output parsing** is off, or while all notifications and all warnings are both disabled globally
            - **Notify**, **Warn** and **Flash** are separate, so a kind can send notifications without showing its chip, and the other way round
        - Clear warnings after (minutes, 0 = never) - How many minutes after a warning's latest trigger its chip is cleared, from 0 up. It applies to the restart, interrupt, error and stalled warnings, and the count is kept when a chip is cleared. 0 never clears them, and is different from leaving the field blank. Save changes is unavailable until it is blank or a whole number of at least 0 (see [Restart warnings](#restart-warnings), [Output warnings](#output-warnings) and [Recording file checks](#recording-file-checks))
        - Warning threshold (restarts) - How many restarts a streamer needs before the `RESTARTED N×` chip is shown, from 1 up. Save changes is unavailable until it is blank or a whole number of at least 1
        - A hint under these two fields says that restarts are counted based on **Consider a stream ended after (minutes)** in Settings > Livestreams
        - Warn if file doesn't exist after (seconds) - How many seconds after the recording file is verified the file must exist, from 1 up. Save changes is unavailable until all four recording file times are blank or valid (see [Recording file checks](#recording-file-checks))
        - Warn if file doesn't start growing after (seconds) - How many seconds after the file is verified before it must start growing, from 1 up
        - Warn if file stops growing for (seconds) - How many seconds the file may go without growing before the recording is marked `STALLED`, from 1 up
        - Check for growth every (seconds) - How often the recording file is checked, in seconds (minimum 5)
        - Enable command output parsing - Sends the recorder's output to be scanned for the patterns. Shown at the left of the row that holds the two pattern boxes. While it is off, the two pattern boxes are shown blank and greyed out and cannot be edited, and their text is kept and shown again when it is turned on. It is also shown off and cannot be changed while both **Disable all notifications globally** and **Disable all warnings globally** are on, without changing the saved choice. It also decides whether the expanded recorder card on the Processes tab shows an **Output** panel (see [Output warnings](#output-warnings) and [Process panel](#process-panel-1))
        - Interrupt patterns (separate with |) - The phrases that make a line of the recorder's output count toward the `INTERRUPT N×` chip and send the interrupt notification, separated with `|` and matched in any upper or lower case. The box is required while the **Notify** or **Warn** switch of the **Interrupts** row is on, and has a red border while it is blank, and **Save changes** is unavailable until it has a value. It is shown blank and greyed out, and cannot be edited, while both switches are off. A blank field is not replaced by a default (see [Output warnings](#output-warnings))
        - Error patterns (separate with |) - The phrases that make a line of the recorder's output count toward the `ERROR N×` chip and send the error notification, in the same way. It is shown beside **Interrupt patterns**, in one row. Both fields apply from the next line of output
        - Save changes button - Saves your edits. Unavailable while there are no edits, or while the draft is not valid
        - Revert button - Discards your edits
        - Defaults button - Fills the page with the notification and warning settings a site starts with, which are those of the site template with the same name as the site, or the **New site** settings for any other name. The other Site card settings are not changed. Nothing is saved until you press **Save changes**, and **Revert** brings back the saved values
        - unsaved changes - A chip shown beside the buttons while the draft is different from what is saved
    - Posts sub-page
        - Site list - Shown at the top of the sub-page. Chooses the Posts site whose settings are shown
        - Edits are held as a draft until you click **Save changes**. The draft is the same one as in the Posts site card, so an edit made in either place is shown in both
        - Enable Notifications for <site> Posts - The master switch for notifications about new posts on the site (see [Posts](#posts))
        - Desktop Notifications - Shows a desktop notification. Greyed out and shown off while **Enable Notifications** is off. The choice is kept, and shown again when **Enable Notifications** is turned on
        - Mobile Notifications (ntfy) - Sends an ntfy notification to the topic below. Greyed out and shown off while **Enable Notifications** is off, in the same way as **Desktop Notifications**
        - ntfy topic (example: j73hd92xl) - The topic name used for ntfy notifications. The field is greyed out, has no hint text and cannot be edited while **Mobile Notifications (ntfy)** or **Enable Notifications** is off. The field has a red border while both are on and no valid topic is set, and Save changes is unavailable until one is. While the field can be edited and is blank, its hint text shows the global ntfy topic, which the site then uses
            - **Randomize** button - Shown beside the field. Fills it with a new random topic. Greyed out while the field cannot be edited. Nothing is saved until you press **Save changes**
        - Save changes button, Revert button and unsaved changes chip - Work in the same way as on the Livestreams sub-page
    - General sub-page - Applies to every site, Livestreams and Posts. Changes apply immediately, and are saved in `settings.json` (see [Settings](#settings))
        - Disable all notifications globally - Stops every notification, without changing any site's own settings. The notification controls on the other sub-pages are greyed out while it is on (see [Notifications](#notifications))
        - Disable all warnings globally - Hides every warning chip, without changing any site's own settings. The **Warn** controls on the Livestreams sub-page are greyed out while it is on. Notifications are not affected
        - Global ntfy topic (example: j73hd92xl) - The ntfy topic used by every site whose own topic is blank. A topic can use letters, digits, `-` and `_`, up to 64 characters, and it can be left blank. The **Save** button, or Enter, saves it, and the field has a red border and **Save** is unavailable while a topic that is not blank is not valid
            - **Randomize** button - Fills the box with a new random topic of 12 lowercase letters and digits. Nothing is saved until you press **Save**
            - **Revert** button - Puts the saved topic back in the box. Unavailable while the box is the same as the saved topic
            - While no topic has been saved, jj-dlp generates a random topic when it starts and saves it right away, so sites with a blank ntfy topic can use it

- Livestreams page
    - Site list - Chooses the site whose Site card is shown. Shown at the top of the page, above the Site card
        - It lists every site you have set up and every site template that has no site yet, followed by **New site**
        - Site templates - Pre-created settings for some popular sites. A template does not need to be chosen before streamers can be added. Its site is created when the first streamer is added to it from the Livestreams tab, and choosing the template here is for editing the site's settings first (see [Livestreams tab](#livestreams-tab))
        - A template fills in the site name, URL template, checker arguments, recorder arguments, site color and the interrupt and error patterns. Every other field is left blank, so it uses the default (see [Site defaults](#site-defaults))
        - Choosing a template that has no site yet adds the site and shows its Site card
        - Choosing a template for a site that already exists does not add a second site (names are compared ignoring case). The Site card of the existing site is shown instead
        - While no site is chosen, such as before any site has been added, the list shows `Site` and no Site card is shown
        - **New site** opens an empty Site card. The card is not saved until you add it
            - **Add site** button - Adds the site. Unavailable until the card has a site name and a URL template, and every field is valid
            - **Discard** button - Closes the card without adding a site
            - Choosing **New site** again keeps the card you are editing
            - After the site is added, its Site card is shown
            - The **Revert** and **Defaults** buttons are not shown on the blank card
																																																														   
        - Each template comes with its own site color
																		   
    - Site card
        - The fields are arranged in compact rows, so all of a site's settings fit in the default window size. The site name and URL template labels include an example, and the checker arguments and recorder arguments labels include a short description of their placeholders
        - A field that can be left blank shows its default as grey hint text (see [Site defaults](#site-defaults))
        - Site name (example) - The display name for the site. Required, with no default. The field has a red border while it is blank, and Save changes is unavailable until it has a name
        - URL template (example) - The stream address for the site, with `{username}` where the streamer's name goes. Set this before adding streamers to the site. Required, with no default. The field has a red border while it is blank, and Save changes is unavailable until it has a value
        - Check every (seconds) - How often to run the yt-dlp checker command for that site, in seconds, from 5 up (default 60). The time is counted from when the previous check has finished, so a slow check does not shorten it. Save changes is unavailable until it is blank or a whole number of at least 5. A streamer can set its own value in its settings (see [Streamer settings](#streamer-settings))
																														  
        - Consider a stream ended after (minutes) - How many minutes the checker must go without seeing the streamer live before the live time is cleared, from 1 up (default 30). Save changes is unavailable until it is blank or a whole number of at least 1 (see [Live time](#live-time))
																																																																																									
																																																				  
        - The restart, warning, file check, notification and pattern fields of a site are on the Notifications page (see below). **Consider a stream ended after (minutes)** stays here, and also sets how soon after an unexpected exit a new recorder counts as a restart (see [Restart warnings](#restart-warnings))
																																																																																																																																																													  
																																																														
																																															 
																																																																													 
																																																																													 
        - Site color - The color of the site's name and tags (see [Site color](#site-color)). The hex field has a red border and Save changes is unavailable until the hex value is valid
            - Swatch - Shows the color that will be used
            - Hex field - The color as `#RRGGBB`, with or without the `#`. It is saved in uppercase. Disabled while **Use theme color** is on
            - Use theme color - Uses the accent color of the current theme instead of a chosen color
            - Preset swatches - A row of colors to click. Clicking one fills the hex field and turns **Use theme color** off. The swatch that matches the current color is outlined
																																																																			 
																															   
																																						  
																							   
																																										
																																												  
																																								
																								 
																																																							 
																																																													
																																													   
																																																																																	   
																																									  
														
																					   
																										   
																																							 
        - Name restarted recordings _partN - Names a recording that starts again during the same stream with the next `_partN` (see [Auto-suffix](#auto-suffix)). A streamer's own setting can override it. Shown beside **Consider a stream ended after (minutes)**
        - Path to yt-dlp executable - The full path to yt-dlp, just `yt-dlp` if it is on your `PATH`, or a relative path such as `bin\yt-dlp\windows\yt-dlp.exe` (see [yt-dlp path](#yt-dlp-path)). Leave blank to use the bundled yt-dlp for your platform. The hint shows the path that will be used
        - Checker arguments - The yt-dlp arguments or options to pass when checking for livestreams. Leave blank to use `--dump-json {urls}`
            - The output format is detected from the arguments. `--dump-json` (or `-j`) reads one JSON line per live stream, and `--simulate` (or `-s`) reads the lines printed by `--print`. If both are present, `--dump-json` is used
            - A warning is shown under the field when the arguments have neither, because no live streams can be detected without one of them (see [Problems](#problems))
        - Recorder arguments - The yt-dlp arguments or options to pass when downloading a livestream. Leave blank to use the default`
            - A warning is shown under the field when `--no-part` is missing, because the recording file cannot be verified without it. Not shown while the field is blank, because the default has `--no-part` (see [Recording file checks](#recording-file-checks))
        - Output folder - The path to save the livestream file. Leave blank to use the global output folder, and the hint shows the full path of the global output folder
            - **Browse** button - Shown beside the field. Opens your system's folder dialog, which starts in the folder that is used now. Choosing a folder fills the field, and nothing is saved until you press **Save changes**
            - **Open** button - Shown beside the **Browse** button. Opens the folder in your file manager, and creates it if it does not exist. It opens the global output folder while the field is blank
        - Subfolders - The subfolders each recording is saved in, below the output folder (see [Recording subfolders](#recording-subfolders))
            - Choose `No subfolder`, `streamer\`, `site\`, `streamer\site\` or `site\streamer\` (default `streamer\`)
            - Streamers use this layout unless their own settings choose another one (see [Streamer settings](#streamer-settings))
            - Effective path - Shows the full path a recording will be saved to, with `<streamer>` in place of the streamer's name (example: `Effective path: C:\Users\owner\Videos\jj-dlp\Twitch\<streamer>\<filename>.mp4`)
            - A warning is shown under the path when the recorder command has an absolute `--output` path, which stops subfolders from applying
        - Split recordings - Splits recordings of the site's streamers into parts (see [Split recordings](#split-recordings)). A streamer's own setting can override it
        - Split the recording after (minutes) - How many minutes each part runs, from 1 up. The field has a red border and Save changes is unavailable while **Split recordings** is on and the field is blank. Save changes is also unavailable while it holds anything other than a whole number of at least 1. Typing a value turns **Split recordings** on, and turning **Split recordings** off clears the field
																																																		  
        - Upgrade quality when available - Restarts a recording once when the stream offers a higher quality than the one being recorded (see [Upgrade quality](#upgrade-quality)). A streamer's own setting can override it. Shown under **Split recordings**
        - Restart stalled recordings - Stops a recording that has become `STALLED` and starts it again, up to a limit for each live time (see [Restart stalled recordings](#restart-stalled-recordings)). Shown under **Upgrade quality when available**
        - Quick check ended streams - Checks a streamer again on its own shortly after its recording ends (see [Quick check](#quick-check)). A streamer's own setting can override it. Shown under **Restart stalled recordings**
        - Quick check after (seconds) - How many seconds after a recording ends the quick check is made, from 1 up. A blank field uses the fallback shown as hint text. Save changes is unavailable until it is blank or a whole number of at least 1. Shown beside **Quick check ended streams**
        - Pause checking - Stops the site's streamers from being checked, so no new checks run for the site until it is turned off again
            - Recordings that are already running carry on, and a `PAUSED` chip is shown in the site header on the Livestreams tab (see [Livestreams tab](#livestreams-tab))
            - It applies when the site is saved, and checking starts again right away when it is turned off
																															 
																							   
																																								
																																														   
        - Save changes button - Saves your edits. Unavailable while the site name or URL template is blank, or any field is invalid
																																																													 
        - Revert button - Discards your edits
        - Defaults button - Fills the card with the default settings. Nothing is saved until you press **Save changes**, and **Revert** brings back the saved values
            - Everything except **Site name** is reset. The timings, yt-dlp path and other optional fields are cleared, so they use the defaults
            - A site named after an existing site template gets that template's settings, including its site color and patterns. Any other site gets the **New Site** settings, with blank URL template and arguments
            - **Interrupt patterns** and **Error patterns** are filled in from those settings instead of being cleared
            - Uses the current defaults, so it also applies any new defaults added in later versions

- General page
    - Folders card
        - Global output folder - The path to save livestream files for sites that have no output folder of their own. Press **Save** or Enter to apply it
            - The hint text shows the folder that is used while the field is blank
            - **Browse** button - Shown beside the field. Opens your system's folder dialog, which starts in the folder that is used now. Choosing a folder fills the field, and nothing is saved until you press **Save**
            - **Open** button - Shown beside the **Browse** button. Opens the folder in your file manager, and creates it if it does not exist. It opens the folder that is used while the field is blank
            - Spaces before and after the path are ignored
            - The folder cannot be blank. Saving a blank field puts the standard output folder back
    - Disk space card - Lists the drives that are tracked and lets you add more (see [Disk space](#disk-space)). Shown under the Folders card. Changes apply immediately
        - A short description is shown under the card title
        - **Show percentage** - Adds the percentage free to the header chips and to the notifications
        - **Show free** - Adds or removes the word `free` in the header chips and in the notifications
        - Low limits - Two boxes, shown as `Mark a drive low below [ ] % free or [ ] GB free`. Only digits can be typed, and the percentage is at most 100. A blank box uses the fallback shown as hint text
        - Drive rows - One row for each tracked drive, with its name, its free space out of its total size with the percentage free, and what uses it. The free space is shown in the warning color while the drive is low
            - Used by - The output folders that are on the drive, shown as **Global**, the name of a livestream site, or `Posts:` followed by the name of a Posts site
            - **Added manually** is shown for a drive you added that no output folder is on
            - **Not connected** is shown in place of the space for an added drive that is not mounted right now
        - **▲** and **▼** buttons - Shown at the right of every row. Move the drive one place up or down in the list. **▲** is unavailable on the first row and **▼** on the last. The order is used for the header chips (see [Disk space](#disk-space))
        - **Remove** button - Shown on the drives you added. The drive stops being tracked unless an output folder is still on it, and is then shown without the button
        - **Add a drive...** list - Lists the connected drives that are not tracked yet, with their free space. Choosing one adds it right away. It is not shown when there is nothing to add
        - The drives you add are saved (see [Settings](#settings))
    - Compact view card - Has a **FULL VIEW** section and, under it, a **COMPACT VIEW** section. Changes apply immediately
        - **FULL VIEW** section - Sets when the normal view uses two columns (see [Livestreams tab](#livestreams-tab))
            - Column Width Min - The narrowest a column may be, from 300 to 1300 px in steps of 10 (default 850). The streamer rows of each site panel are split into two columns when the window is at least twice this wide, and use one column when it is narrower. It is separate from the **Column Width Min** of the **COMPACT VIEW** section
            - Reset button - Shown in the section header. Restores the width to its default
            - The value is saved when you release the slider
        - **COMPACT VIEW** section - Sets how compact view sizes its rows, how many columns it uses and how long streamer names can be (see [Compact view](#compact-view))
        - Maximum - The largest row height, from 15 to 48 px (default 31). Rows use this size while there is room
        - Minimum - The smallest row height, from 15 to 48 px (default 15). Rows never shrink below this size
        - Column Width Min - The narrowest a column may be, from 300 to 1300 px in steps of 10 (default 540). Compact view uses as many columns as fit at this width, up to three, and one column when the window is narrower than two columns
        - Name length - How many characters of a streamer name are shown before it is cut off with `…`, from 6 to 30. Hovering a shortened name shows the full name
																		   
        - Maximum and Minimum stay in order: moving one past the other moves the other one with it
        - Reset button - Shown in the header of the **COMPACT VIEW** section. Restores all four values to their defaults, and does not change the **FULL VIEW** section
        - Values are saved when you release the slider
    - Display card - Sets the scale of the whole window, so jj-dlp looks the same on small laptop screens and large monitors. Changes apply immediately
        - Automatic UI scale - Picks the scale from the size of the monitor the window opens on, measured in the pixels the system gives to programs (default on). A 1920 × 1080 monitor is 100%, and the scale follows the monitor size from 80% to 150% in steps of 5%
        - While it is on, the card shows the scale that is in use. The scale is checked each time jj-dlp starts, and the window is resized to the new scale the first time it changes
        - Scale - Shown while **Automatic UI scale** is off. Sets the scale by hand, from 70% to 200% in steps of 5% (default 100%). It is applied when you release the slider
        - The scale applies to the text, rows, buttons and the window size together, and the sizes in the **Compact view** card and the Window card are measured at the scale
        - Reset button - Shown in the card header. Turns **Automatic UI scale** on and sets the manual scale to 100%
        - On Wayland the desktop places the window, so it is not moved back to the center when the scale changes
        - The settings are saved right away
    - Updates card - Sets how jj-dlp checks for new releases (see [Updates](#updates)). Changes apply immediately
        - Check for updates automatically - Turn off to stop jj-dlp from contacting GitHub on its own (default on)
        - Include release candidates - Also offers pre-release versions such as `2.0.1-rc.1` (default off)
        - What's new button - Shown in the card header. Opens the changelog (see [Changelog](#changelog))
        - Check now button - Shown in the card header. Looks for a new release right away, even when automatic checks are off
        - The running version is shown under the toggles
    - Window card - Sets whether jj-dlp remembers the window size and resets it to the default (see [Settings](#settings)). Changes apply immediately
        - Remember window size - Reopens jj-dlp at the size the window had when it was last closed (default off). Turning it on saves the current size right away
        - The size is saved when jj-dlp closes. If the app is killed, the size saved before is used
        - With the toggle off, the window opens at the standard size
        - A remembered size is never smaller than 640 × 420 px
        - Only the size is remembered, not the position, so the window always opens centered on the screen
        - A window that would not fit the screen, such as with a large display scale setting, opens smaller, leaving room for the title bar and the taskbar, and stays centered
        - Reset button - Shown in the card header. Resizes the window to the standard size right away. If **Remember window size** is on, that size is also saved
    - Log card - Sets how many messages the Log tab keeps (see [Log tab](#log-tab)). Changes apply immediately
        - Buffer Length - How many messages the Log tab shows, from 100 to 5000 in steps of 100 (default 500)
        - Values are saved when you release the slider
        - Lowering it removes the oldest messages from the tab. The saved history in `ui.log.txt` is not changed
    - The Compact view card is shown beside the Display, Updates, Window and Log cards when the window is at least 1000 px wide. When it is narrower, the cards are stacked

		   
											   
																																															  
																										
																					   
																																															  
																							 
																							  
																									   
																												  
																					
											
										 
																
																													   
													  
- Edits are held as a draft until you click **Save changes**
																	  
- Command arguments
    - Split on spaces. Use double or single quotes for values containing spaces
    - Backslashes are kept literally, so Windows paths work as written. The exception is a backslash before a double quote inside double quotes, which keeps the quote as part of the value
    - The same rules apply to the arguments of a Posts site (see [Posts site settings](#posts-site-settings))
    - The command is run directly, not through a shell
    - A blank checker or recorder command uses its default (see [Site defaults](#site-defaults))
    - `{urls}` - In the checker command, replaced by every streamer URL of the site, one argument each. If it is left out, the URLs are added at the end
    - `{url}` - In the recorder command, replaced by the streamer's stream URL. If it is left out, the URL is added at the end
    - jj-dlp adds its own `--print-to-file` options to the end of the recorder command when a recording starts, for the recording file checks (see [Recording file checks](#recording-file-checks))
    - jj-dlp also adds an `--output` option to the end of the recorder command when recording subfolders are used, and from the second part of a recording (see [Recording subfolders](#recording-subfolders) and [Split recordings](#split-recordings))
- Recorders run with the output folder as their working directory, so relative `--output` templates save there
    - If the global output folder is blank in a settings file edited by hand, and the site has no output folder, recordings are saved in the folder jj-dlp was started from
    - With recording subfolders, the subfolders are added in front of the `--output` template, so recordings are saved inside them (see [Recording subfolders](#recording-subfolders))
- The output folder is created if it does not exist
- URL template
    - Each site has a URL template, which is the stream address with `{username}` standing in for the streamer's name
    - Adding a streamer saves the full URL built from the template, so changing a template later does not change streamers that were already added

## Site defaults

- Blank fields - Most Site card fields can be left blank. A blank field uses the default shown as grey hint text in the field
    - The default is applied each time it is needed and is not saved in the site, so a default changed in a later version applies to every site that leaves the field blank
    - A value typed in the field is used instead of the default
    - **Site name** and **URL template** have no defaults and start blank. **Output folder** uses the global output folder when blank (see [Settings tab](#settings-tab))
    - **Interrupt patterns** and **Error patterns** are the exception. A blank field is only possible while both switches of its row are off, and it is not replaced by a default (see [Output warnings](#output-warnings))
- Site templates only fill in the site-specific fields, so a site made from a template uses the defaults for everything else (see [Settings tab](#settings-tab))

## yt-dlp path

- A site with a blank **Path to yt-dlp executable** uses the bundled yt-dlp for your platform
    - Windows - `bin\yt-dlp\windows\yt-dlp.exe`
    - Linux - `bin/yt-dlp/linux/yt-dlp_linux`
    - Other systems - `yt-dlp`, found through your `PATH`
- A relative path is searched in this order, and the first file found is used
    - The folder containing jj-dlp
    - Up to three folders above it
    - The current working folder
- This works the same whether you start jj-dlp from its own folder or from another one
- `/` and `\` are both accepted in a relative path
- Absolute paths and plain names such as `yt-dlp` are used as written
- Linux - If the bundled yt-dlp is not executable, jj-dlp sets the executable bit and writes a message to the Log tab. The Posts tab does the same for its yt-dlp, and writes a message to the Log tab too

## gallery-dl path

- A site with a blank **Path to gallery-dl executable** uses the bundled gallery-dl for your platform
    - Windows - `bin\gallery-dl\windows\gallery-dl.exe`
    - Linux - `bin/gallery-dl/linux/gallery-dl.bin`
    - Other systems - `gallery-dl`, found through your `PATH`
- A relative path is searched, and Linux files are made executable, in the same way as for yt-dlp (see [yt-dlp path](#yt-dlp-path))

## Process panel

- Recorder card
    - State - `STARTING`, `RUNNING`, `STOPPING` or `EXITED · CONFIRMING`, with a red `STALLED N×` chip added when the recording file is missing or has stopped growing
    - Uptime, the site, and the full command line, wrapped onto extra lines when it is long. The command line includes the options jj-dlp adds for the recording file checks, for recording subfolders and for recording parts
    - bootloader - Shown when yt-dlp is a PyInstaller binary, which launches a second copy of itself
    - yt-dlp - The real yt-dlp process (the deepest process with the same name as the one launched)
    - ffmpeg - Each ffmpeg child, anywhere in the tree
    - Latest output line - The latest line of yt-dlp output. The collapsed card shows it as a single line, cut off at the edge of the card. The expanded card shows it in its own **Last line** panel
- Expanded card - The expanded view of a recorder card. The panels are shown in this order
    - **Output** panel - Shown while **Enable command output parsing** is on for the site and the recorder has printed at least one line (see [Output warnings](#output-warnings))
        - Shows the full output of the recorder, which is every line of its stdout and stderr, oldest first
        - Only complete lines are shown. Progress lines that are rewritten in place are not, and are only shown in the **Last line** panel
        - Keeps the newest 500 lines of each recorder, and the oldest lines are dropped. A very long line is cut off
        - Has a fixed maximum height and scrolls. It follows the newest line while you are at the bottom, and stays where it is once you scroll up, until you scroll back to the bottom
        - **Wrap** switch - Wraps long lines. With it off, long lines stay on one line and the panel also scrolls sideways. It is saved, and applies to every recorder card
        - **Copy** button - Copies the whole output to the clipboard
        - The output is removed when the recorder card is removed
        - Turning **Enable command output parsing** on or off applies to recorders that start afterward. A recorder that is already running keeps its earlier setting
    - **Last line** panel - Shown below the **Output** panel. While **Enable command output parsing** is off, or before any line has been printed, it is shown without the **Output** panel
    - The full command line
- A card is removed only after the launched process and every child seen in its tree have exited
- If stray children linger 20 seconds after the main process exits, they are killed

## Stopping behaviour

- When a recorder is stopped (pause, remove, or quitting the app), how it is stopped depends on your platform
													  
    - Linux - `SIGINT` to the recorder's process group so ffmpeg can finish the file, then `SIGKILL` if it is still running after 20 seconds
    - Windows - `Ctrl+C` to the recorder, sent by a short helper process that jj-dlp starts, so ffmpeg can finish the file, then `taskkill /T /F` if it is still running after 20 seconds, which ends yt-dlp and ffmpeg together. If the `Ctrl+C` cannot be sent, `taskkill /T /F` is used right away
- Closing the window stops all recorders before the app exits
- If the app itself is killed, running recorders keep going until jj-dlp starts again, which stops them (see [Recorder cleanup](#recorder-cleanup))

## Updates

- Update check - Runs at launch and every 6 hours, and again whenever you change a toggle in the Updates card
    - Only a version higher than the running one is offered
    - A failed check is written to the Log tab
    - Check now writes `jj-dlp vX is up to date` to the Log tab when there is nothing newer
- Update banner - Shown above the tabs as **Update available!**, with the new version and the running version
    - `(release candidate)` is shown after the version when the release is a pre-release
    - **Update** button - Asks for confirmation before anything is changed
    - Confirmation - Shows how many recordings will be stopped, with **Install & restart** and **Cancel** buttons
    - Turning off automatic checks, or turning off release candidates while a release candidate is offered, removes the banner
- Install steps
    - The release is downloaded and checked against its SHA-256 file while recordings keep running
    - The window closes and all recorders are stopped (see [Stopping behaviour](#stopping-behaviour))
    - jj-dlp is replaced and starts again, with a new watcher (see [Watcher](#watcher))
    - The new version applies the settings resets that came with the release, if there are any (see [Settings resets](#settings-resets))
- If the download or the check fails, nothing is stopped, and the banner shows `Last attempt failed:` with the reason
- If the final install step fails, the old version starts again and the reason is written to the Log tab
- What is installed
    - Windows and Linux - The jj-dlp executable is replaced, and the `bin` folder in the release is copied over the `bin` folder next to jj-dlp. Files with the same name are overwritten, and other files are left alone
    - macOS - Only the jj-dlp executable is replaced. yt-dlp is found through your `PATH`
- Leftover files - Windows - Files named `.jj-dlp.<random>.__selfdelete__.exe` or `.jj-dlp.<random>.__relocated__.exe` can stay in the jj-dlp folder after an update if they fail to be deleted. jj-dlp tries to delete them at every start, retrying for about a minute while they are still in use by the previous version
- Updates only install into a copy of jj-dlp extracted from a release archive
    - A copy running from a Cargo `target` folder is a development build and is never updated
    - Windows and Linux - A `bin` folder must be next to the jj-dlp executable
- Release builds - Windows (64-bit), Linux (64-bit) and macOS (Apple Silicon)
- Release candidates - A release candidate is only offered when **Include release candidates** is on. Once the final version is released, it is offered to release candidate users even with the toggle off
- `update.json` - Holds `enabled` (default `true`) and `include_rc` (default `false`). Unlike `settings.json`, `sites.json`, `streamers.json` and `state.json`, it is read again at every check, so edits made by hand are picked up without a restart

## Changelog

- What's new window - Lists the changes of every version, newest first, over a dimmed window. The changelog is built into jj-dlp, so it needs no download and works offline
- When it opens - At launch, when the running version is different from the version whose changelog was last shown. This covers updates from the banner, `jj-dlp --update` and manual installs
    - A new install does not open it
    - A version that was installed before this feature existed opens it once
    - It opens for release candidates too
- **Close** button - Closes the window and records that this version's changelog was shown. If jj-dlp is closed without pressing it, the window opens again at the next launch
- What's new button - In the Updates card in Settings. Opens the window again at any time
- Open Changelog button - In the Log tab, beside the Open Log button. Opens the same window
- `changelog_seen.json` - Holds `version`, the version whose changelog was last shown. Deleting it while the other files exist makes the window open at the next launch (see [Settings](#settings))

## Settings resets

- Settings resets - A release can set chosen settings back when jj-dlp starts for the first time in that version, so a value saved by an earlier version that is no longer right is fixed without editing files by hand
    - Applied once, at launch, before anything is checked or recorded. This is the first launch after an update (see [Updates](#updates)), whether it was installed from the GUI or with `jj-dlp --update`
    - Applies in GUI mode, `--no-gui` and `--engine-only` (see [Command-line modes](#command-line-modes)). `jj-dlp --update` does not apply them, because the new version applies them the next time it starts
    - When versions were skipped, the resets of every skipped version are applied, and each reset is applied only once
    - A new install has nothing to reset
- What can be reset
    - The global output folder
    - Any field of the Site card. A reset applies to every site, or only to one built-in site (see the Built-in sites item below)
    - Any field of a Posts site except its name, its accounts and **Pause checking**. A reset applies to every Posts site, or only to one built-in Posts site, and the posts library is never changed
    - The streamer settings for logging and recording subfolders. A reset applies to every streamer
    - The update settings (see [Updates](#updates))
    - The theme, the Compact view settings, the Remember window size setting and window size, and the Log tab buffer length (see [Settings tab](#settings-tab))
    - Site names, streamers and the list of sites and Posts sites are never changed by a reset
- How a setting is reset
    - A field that can be left blank is set back to blank, so it uses the default again (see [Site defaults](#site-defaults)). Any other setting is set back to its default value
    - Checker arguments, recorder arguments, URL template and site color are set to the settings of the site template the site was made from (see [Settings tab](#settings-tab) and the Built-in sites item below). A site that is not a built-in site is left as it is
    - For a Posts site, the checker arguments, download arguments, URL template, **Twitter/X Helper**, **Use TikTok Photos scraper**, automatic download, size and time limits, backfill, **Open m3u8 instead of site** and automatic delete settings are set to the settings of the site template the Posts site was made from (see [Posts site settings](#posts-site-settings) and the Built-in sites item below). A Posts site that is not a built-in site is left as it is. **Check every** is set to the template's value for a built-in site, and to the default for any other site. Every other Posts site field is set back to its default
    - A reset either replaces whatever value is saved, or only replaces a value that is still the specific old value the release names. In the second case, a value you changed to something else is left alone
    - A setting that already has the value it would be reset to is not changed
- Log tab - One message is written for each setting that was changed, right after the `--- session started (vX) ---` message (see [Log tab](#log-tab))
    - The message starts with where the setting is: `settings`, `site '<name>'`, `posts site '<name>'`, `streamer '<name>'`, `update.json` or `gui.json`
    - It then shows the field name as it is written in the settings files, the old value and the new value (example: `site '<name>' interval_secs reset: Some(45) -> None`)
    - A site that is not a built-in site is skipped for a template-based field, with no message
    - The old values are not kept anywhere else, so the message is the only record of them
- Built-in sites - A site made from a site template, or named like one, is marked with a hidden key so a reset can find it even after it is renamed
    - At every launch, before the resets, a site with no key whose name matches a site template (ignoring case and spaces around the name) gets that template's key. A site named **TikTok** is therefore always a built-in site
    - The key is kept when the site is renamed or edited. A site renamed to a template's name gets the key when it is saved, or at the next launch
    - A message `site '<name>' site_key set: <key>` is written to the Log tab for each site that gets a key
    - A site that is not named like a template has no key and is never changed by a reset that is limited to one built-in site
    - The key is saved as `site_key` in `sites.json` and `posts.json`. A file saved by an earlier version has none, and gets it at the next launch
- If a file cannot be saved, a message `config reset: cannot save <file>` with the reason is written to the Log tab, and the resets are tried again at the next launch
- `config_rev.json` - Holds the number of the last settings resets that were applied (see [Settings](#settings))

## Single instance

- Only one jj-dlp can run at a time for each settings folder, so two copies never record the same streamers or write the same settings files
    - Copies that use different settings folders can run together (see [Command-line modes](#command-line-modes))
    - Applies to GUI mode, `--no-gui` and `--engine-only`, which also block each other
    - `--update` does not take part in this check (see [Command-line modes](#command-line-modes))
- Starting a second copy - The second copy shows `jj-dlp is already running.` and exits, and the first copy keeps running
    - Windows, GUI mode - Shown in a message box
    - Other cases - Printed to the terminal, and the exit code is 1
- Watcher - The watcher takes its own lock, so a second copy is stopped by the watcher before it starts anything (see [Watcher](#watcher))
    - The app that the watcher starts keeps the lock described here, and waits up to 15 seconds for it when it is started again, so a crash that has just happened does not show `jj-dlp is already running.`
    - When the app finds another copy already running, it exits with a code that tells the watcher not to start it again
- After an update - The new version waits up to 15 seconds for the old one to exit, so it does not show `jj-dlp is already running.` while the old copy is still closing (see [Updates](#updates))
- The lock is released when jj-dlp exits, including when it is killed or crashes, so nothing needs to be cleaned up by hand
- `instance.lock` - The small file used for the lock, in the config folder (see [Settings](#settings)). It can be deleted when jj-dlp is not running
- `watcher.lock` - The small file the watcher uses for its own lock, in the config folder. It can be deleted when jj-dlp is not running
- If the lock file cannot be opened, jj-dlp starts anyway

## Watcher

- Watcher - Starts jj-dlp as a separate process and starts it again if it exits unexpectedly, so recording carries on after a crash without anyone opening the app
    - Used by GUI mode, `--no-gui` and `--engine-only`. `--update` and `--help` do not use it (see [Command-line modes](#command-line-modes))
    - Two jj-dlp processes are shown while it runs, such as in the Task Manager. The small one is the watcher, which only watches, and the one that uses more memory is the app. The window, the settings and the recorders all belong to the app
    - Every option jj-dlp is started with is passed on to the app, including `--config-dir=<path>`
    - `--no-watcher` runs the app on its own, without a watcher (see [Command-line modes](#command-line-modes))
- When the app is started again - Whenever it ends in any way other than the ones listed below
    - A crash, a panic and the process being killed are all started again, including **End task** on the app's own entry in the Task Manager
    - The window failing to open, or ending with an error, is started again too, and the reason is written to `crash.log`
    - Not started again - The app closing normally, such as closing the window or pressing Ctrl+C with `--no-gui`, and the app finding another copy already running (see [Single instance](#single-instance))
        - On Linux and macOS, an app that is ended with an interrupt, terminate or hangup signal is not started again either
        - The watcher exits too
    - Ending the app and the watcher together, such as **End task** on the whole jj-dlp entry of the Apps list in the Task Manager, stops everything, because nothing is left to start the app again
- Delay - The watcher waits a short time before starting the app again. The wait doubles each time the app ends soon after starting, up to a limit, and starts over once the app has run for a couple of minutes
    - The watcher never gives up
    - Stopping the watcher during the wait stops it without starting the app
- Stopping - Ctrl+C, and a terminate or hangup signal on Linux and macOS, are passed on to the app, which stops its recorders and exits (see [Stopping behaviour](#stopping-behaviour))
    - The watcher waits up to a minute for the app to exit, and then ends it
    - Closing the window stops the recorders and exits, and the watcher exits with it
- If the watcher ends - The app checks every few seconds that its watcher is still running. When it is not, the app stops the recorders it started, writes a line to `crash.log` and exits (see [Recorder cleanup](#recorder-cleanup))
    - Ending the watcher on its own therefore never leaves an app running that nothing is watching
- Single copy - One watcher runs for each settings folder, so a second copy shows `jj-dlp is already running.` and exits (see [Single instance](#single-instance))
- After an update - The app starts the new version and exits, which also ends the old watcher. The new version starts with its own watcher (see [Updates](#updates))
- Crash log - Each time the app exits, the watcher adds a line to `crash.log` with the time, how it ended and how long it ran (see [Crash log](#crash-log))
    - Started again - `Watcher: jj-dlp exited unexpectedly (exit code 101 / 0x00000065 after 86400s); restarting in 2s.` The exit code is shown in decimal and in hex. On Linux and macOS a signal number is shown in place of the exit code
    - Not started again - `Watcher: jj-dlp exited (exit code 0 / 0x00000000 after 600s); not restarting.`
    - A panic, if there was one, is written by the app before its exit line
- Log tab - A started again app writes the message of an unclean previous session, followed by `Restarted automatically by the watcher after jj-dlp exited unexpectedly (<how it ended>).` The first start, and the start after an update, do not write the second message (see [Log tab](#log-tab))
- Not covered
    - An app that has frozen but not exited is not started again
    - A recording that was running when the app crashed is cut off. Once the app is back, a streamer that is still live is recorded again, as a new part while auto-suffix is on (see [Auto-suffix](#auto-suffix))

### Recorder cleanup

- Recorder cleanup - Stops recorder processes that a crash left running, so a started again jj-dlp does not record the same stream twice
    - Works in GUI mode, `--no-gui` and `--engine-only`, with or without the watcher, so it also applies when jj-dlp is started by hand after a crash
    - Nothing is left to clean up after a normal exit, because the recorders are stopped first (see [Stopping behaviour](#stopping-behaviour))
- Tracking - While recorders run, the processes of every recorder, including its yt-dlp and ffmpeg processes, are written to `recorders.json` in the settings folder (see [Settings](#settings))
    - The list is updated as processes start and stop, and is emptied when the last recorder has stopped
    - A recorder that crashed less than about a second after starting may not be on the list yet
- At launch - Before anything is checked or recorded, every process on the list that is still running is stopped
    - A process is only stopped when it is still the same process that was listed, so an unrelated program that has been given the same process number is never stopped
    - On Linux and macOS, the whole process group of a recorder is stopped
- Log tab - `Stopped N recorder process(es) left running by the previous session.` is written to the Log tab and to `crash.log`, only when something was stopped (see [Log tab](#log-tab))

## Config problems

- Config problems - Protect your settings when a settings file cannot be loaded, so a typo in a file edited by hand does not wipe your sites and streamers
    - Checked at launch. The files are `settings.json`, `sites.json`, `streamers.json` and `state.json`, and for the Posts tab `posts.json` and `posts_library.json` (see [Settings](#settings))
    - A file that is missing while the others exist, or that is not valid, is a problem. The first launch, when none of the files exist, is not
- What happens
    - A copy of each existing file is saved next to it with `.bad` added to its name (example: `sites.json.bad`). An earlier `.bad` copy is replaced
    - jj-dlp starts with no sites and no streamers, and does not save anything, so the broken files are not overwritten
    - Edits made while saving is paused are not saved, and a message is written to the Log tab each time a save is refused
    - The settings files and the Posts files are handled separately, so one group can be broken while the other loads
- Banner - Shown above the tabs with the reason and two buttons, and it pulses
    - **Open config folder** - Opens the config folder (see [Settings](#settings))
    - **Start fresh** - Lets jj-dlp save again, replacing the broken files with what is shown now. The `.bad` copies are kept, and one message is written to the Log tab (see [Log tab](#log-tab))
    - The banner stays until you press **Start fresh** or restart jj-dlp with the files fixed. A reload while the app is running does not clear it
- Fixing it by hand - Correct the file, or copy its `.bad` copy back over it, and restart jj-dlp
- `gui.json` - An invalid file is kept as `gui.json.bad`, and jj-dlp carries on without its saved window, theme and Compact view values until a setting is saved again. It does not show the banner or pause saving
- `--no-gui` - Also keeps the `.bad` copies and does not save over the broken files, but has no window to show the banner

## Command-line modes

- `jj-dlp` - Runs the app in GUI mode
- Only one jj-dlp can run at a time. A second `jj-dlp`, `--no-gui` or `--engine-only` exits right away (see [Single instance](#single-instance))
- `jj-dlp --no-gui` - Runs the app without a window. Log messages are printed to the terminal
    - Posts are checked and downloaded too, in the same way as in GUI mode (see [Posts](#posts))
- `jj-dlp --engine-only` - Runs only the basic recording engine, without the settings layer or a window. Log messages are printed to the terminal
    - Posts are not checked or downloaded
    - Every enabled streamer is recorded as soon as they are seen live. Behavior that the settings layer decides, such as the intro delay, schedules, splitting and suffixes, does not apply (see [Intro delay](#intro-delay), [Schedule](#schedule) and [Split recordings](#split-recordings))
- `jj-dlp --config-dir=<path>` - Keeps the settings in `<path>` instead of the OS config folder, so more than one jj-dlp can run at the same time, each with its own settings (see [Settings](#settings))
    - Can be combined with `--no-gui`, `--engine-only` and `--update`
    - Everything that is kept in the settings folder is kept in `<path>`, including the lock that stops a second copy, so copies with different folders do not block each other (see [Single instance](#single-instance))
    - A relative path starts from the folder jj-dlp was started from. The folder is created when it is needed
    - After an update, jj-dlp starts again with the same option (see [Updates](#updates))
    - Each copy has its own global output folder, so set a different one in each copy if the recordings should be kept apart
- `jj-dlp --perf` - Writes timing information to `perf.log` in the settings folder
    - Lists operations in the engine, the window and the file writer that took long, and how many messages the window handled in each few seconds
    - A `gui stalled` line is added whenever the window did not respond for about a second and a half or more, with how long it was
    - Can be combined with `--config-dir`, `--no-gui` and `--engine-only`
    - After an update, jj-dlp starts again with the same option (see [Updates](#updates))
- `jj-dlp --backend=<name>` - Chooses how the window is drawn. The names are `dx12`, `vulkan`, `gl` and `software`, where `software` draws on the processor instead of the graphics card
    - Try another name when the window freezes or stutters, for example when it is minimized
    - A name that is not in the list prints the options and exits
    - Only the GUI mode has a window, so the other modes ignore it
    - Can be combined with `--config-dir` and `--perf`
- `jj-dlp --no-watcher` - Runs the app on its own, without the watcher that starts it again after a crash (see [Watcher](#watcher))
    - Only GUI mode, `--no-gui` and `--engine-only` use the watcher, so the other modes ignore it
    - A crash is then not followed by a new start, but recorders left running are still stopped the next time jj-dlp starts (see [Recorder cleanup](#recorder-cleanup))
    - Can be combined with `--config-dir`, `--perf`, `--backend`, `--no-gui` and `--engine-only`
- `jj-dlp --update` - Installs the latest release and exits, without opening a window or restarting
    - Add `--rc` to include release candidates for that run
    - Refuses to run while another jj-dlp is running. Close it first
    - Works even when automatic update checks are turned off
    - Settings resets that come with the release are applied the next time jj-dlp starts (see [Settings resets](#settings-resets))
- Both headless modes read the settings files at launch and record every enabled streamer (see [Settings](#settings))
    - `--no-gui` applies the intro delay and schedules, and `--engine-only` does not (see [Intro delay](#intro-delay) and [Schedule](#schedule))
- `--no-gui` checks for updates at launch and every 6 hours, and prints `Update available: vX. Run jj-dlp --update to install it.` It never installs an update by itself
- `--engine-only` does not check for updates
- Press Ctrl+C to stop all recorders and exit (see [Stopping behaviour](#stopping-behaviour))
- `jj-dlp --help` lists the options

## Settings

- Settings are saved automatically in the OS config folder, split across `settings.json`, `sites.json`, `streamers.json` and `state.json`
    - The Posts files, `live.json` and the log files are written in the background, so a change reaches the disk a moment after it is made. Anything still waiting is written when jj-dlp exits. A file that cannot be written is reported in the Log tab (see [Log tab](#log-tab))
    - Linux - `~/.config/jj-dlp/`
    - Windows - `%APPDATA%\jj-dlp\config\`
- `--config-dir=<path>` - Uses `<path>` in place of the OS config folder (see [Command-line modes](#command-line-modes))
- Open settings folder button - On the Settings tab, at the far right of the sub-tab row and shown on every sub-tab, opens the config folder
- Files in the config folder
    - `settings.json` - Holds the global output folder, the recording limit and the global notification settings
    - `sites.json` - Holds the sites, including each site's **Stream ended after (minutes)** setting, restart warning settings, command output setting, interrupt and error warning switches, flash switches, interrupt and error patterns, site color, recording subfolder layout, recording file check times, split settings, upgrade quality setting, restart stalled recordings setting, quick check settings and notification settings. A field left blank is saved blank, and its default is applied when it is needed. Each site also has a `site_key` (see [Settings resets](#settings-resets))
    - `streamers.json` - Holds the streamers, including each streamer's log, subfolder, notification, split, auto-suffix, upgrade quality, quick check, intro delay, schedule and always record options, and the time each schedule last switched the streamer on and off. The order of the streamers in the file is the priority order (see [Priority](#priority))
    - `state.json` - Holds the number used for the next site or streamer you add
    - `gui.json` - Holds the theme, the Compact view settings, the Full view column width, the UI scale settings, the **Live first** setting, the **Group by site** setting, the sort option and the sort direction, the remembered window size, the Log tab buffer length, the **Wrap** setting of the Processes tab output, the **Wrap** setting of the Output sub-tab of the Log tab, the Files tab settings, the **Newest first** and card size settings of the Posts page, and the drives added to the Disk space card, their order and the Disk space display settings
    - `file_owners.json` - Holds the streamer and the site that each recorded file came from, used to group the files on the Files tab (see [Files tab](#files-tab))
    - `posts.json` - Holds the Posts sites, with their settings, their `site_key` and their accounts (see [Posts](#posts))
    - `posts_library.json` - Holds the posts that have been found, with whether each is viewed, starred or downloaded and where its file is (see [Posts](#posts))
    - `post_thumbs` folder - Holds the thumbnail of each post (see [Posts](#posts))
    - `live.json` - Holds when each live streamer was first seen live, when each streamer was last seen live, when jj-dlp last looked up last live times on Twitch, which recording part each live streamer is on, and which live streamers have had their quality upgraded (see [Live time](#live-time), [Last live](#last-live), [Split recordings](#split-recordings) and [Upgrade quality](#upgrade-quality))
    - `update.json` - Holds the update settings (see [Updates](#updates))
    - `changelog_seen.json` - Holds the version whose changelog was last shown (see [Changelog](#changelog))
    - `config_rev.json` - Holds the number of the last settings resets that were applied. Deleting it while the other files exist makes every settings reset apply again at the next launch (see [Settings resets](#settings-resets))
    - `*.bad` files - Copies of files that could not be loaded, such as `sites.json.bad` (see [Config problems](#config-problems))
    - `ui.log.txt` - Holds the saved history of the Log tab (see [Log tab](#log-tab))
    - `instance.lock` - Used to stop a second jj-dlp from running (see [Single instance](#single-instance))
    - `watcher.lock` - Used to stop a second watcher from running (see [Watcher](#watcher))
    - `recorders.json` - Holds the processes of the running recorders, so recorders left running by a crash can be stopped (see [Recorder cleanup](#recorder-cleanup))
    - `crash.log` - Holds the panics that were recorded, reports of previous sessions that did not exit cleanly, and how the watcher saw jj-dlp exit (see [Crash log](#crash-log))
    - `session.json` - Exists while jj-dlp is running and is removed when it exits cleanly (see [Crash log](#crash-log))
    - `perf.log` - Written only when jj-dlp is started with `--perf` (see [Command-line modes](#command-line-modes))
    - `logs` folder - Holds the log files (see [Log files](#log-files))
    - `sidecars` folder - Holds one small file per recording streamer, used by the recording file checks (see [Sidecar files](#sidecar-files))
    - `post_sidecars` folder - Holds a small file for each post that is downloading, used to find the downloaded file (see [Posts](#posts))
- The default output folder is `jj-dlp` inside your Videos folder
- Linux - At launch jj-dlp also writes `jj-dlp.desktop` to `~/.local/share/applications` and an icon file to `~/.local/share/icons`, so the desktop can show the window icon. The launcher entry is updated when the jj-dlp executable moves, and nothing is written when a `jj-dlp.desktop` is already installed for all users
- `settings.json`, `sites.json`, `streamers.json` and `state.json` are read at launch. Edits made to them by hand while the app is running are not picked up until the next launch
- All four files must be present and valid. If one is missing or invalid at launch, jj-dlp keeps a `.bad` copy of each file, shows a warning banner and does not save until you choose what to do (see [Config problems](#config-problems)). A reload while the app is running keeps the current settings and writes the reason to the Log tab
- When none of the files exist yet, which is the first launch, jj-dlp starts with no sites and no streamers (see [Livestreams tab](#livestreams-tab))

## Crash log

- `crash.log` - In the settings folder (see [Settings](#settings)). A panic on any thread adds an entry with the time, the version, the name of the thread, where in the code it happened, the message and a backtrace
    - The entry is written right away, before jj-dlp stops
    - On Windows, the backtrace only shows function names when the `.pdb` file is next to the executable
    - The file is emptied before the next write once it is over 1 MB
- Unclean exits - While jj-dlp runs, `session.json` in the settings folder records the process id, the version, when the session started and when it was last seen alive, and is updated every 30 seconds. A normal exit, including the restart after an update, removes it
    - When jj-dlp starts and finds `session.json`, the previous session ended without a clean exit. A line with those values is added to `crash.log` and to the Log tab (see [Log tab](#log-tab))
    - A `PANIC` entry before that line means a Rust panic. Without one, the process was killed, crashed outside of Rust code, such as a graphics driver fault, or the PC restarted or went to sleep
    - On Windows, Event Viewer, under Windows Logs, **Application** and **System**, shows crashes and restarts around the last-seen time
    - Applies to GUI mode, `--no-gui` and `--engine-only`. `--update` does not write it
    - Copies with different settings folders each have their own (see [Command-line modes](#command-line-modes))
- A panic while a file is being written in the background is recorded in the same way, and later writes carry on
- Watcher entries - The watcher adds a line each time the app exits, with how it ended and whether it is started again (see [Watcher](#watcher))
    - The exit code is the quickest way to tell a panic from a crash outside of Rust code. A panic ends with exit code 101, and a crash in a driver or library usually ends with a long code such as `0xC0000005`
    - A window that ends with an error is written as `GUI error: <reason>` before the exit line
    - `Watcher is gone; stopping recorders and exiting.` is written by the app when its watcher has ended (see [Watcher](#watcher))
    - `Stopped N recorder process(es) left running by the previous session.` is written when recorders left by a crash are stopped (see [Recorder cleanup](#recorder-cleanup))
- After a crash, the app that the watcher starts finds `session.json` and writes the unclean session line described above, so each crash is followed by that line and then by a restart message in the Log tab

## Log files

- Output logging is off by default
- Log files are written to the `logs` folder inside the settings folder
					 
- `<site>.checker.stdout.log` and `<site>.checker.stderr.log` - One pair per site. Each check adds the timestamp, the command line and the exit code and duration, followed by the output of that stream, to the file of each stream you enabled. Both files get the timestamp line, so each file can be read on its own
- `posts.<site>.<account>.checker.stdout.log` and `posts.<site>.<account>.checker.stderr.log` - One pair per Posts account. Each command run for the account, which is its list of posts and any extra call for a post's details, adds the timestamp, the command line and the exit code and duration, followed by the output of that stream, to the file of each stream you enabled
- `posts.<site>.downloader.stdout.log` and `posts.<site>.downloader.stderr.log` - One pair per Posts site, in the same format. Each download, and each look-up of a post's stream link, adds an entry
- A command that times out or cannot be started adds the timestamp, the command line and the reason, such as `timed out after 30s`, instead of output, to the file of each stream you enabled. This applies to the checker of a site, to the checker of a Posts account and to the downloader of a Posts site
- `twitch.streamer_username.stdout.log` and `twitch.streamer_username.stderr.log` - One pair per streamer. Each line is timestamped in your local time. A recording adds a `start:` line (command line, working folder, process id) and an `exit:` line (exit code, runtime)
- Turning a recorder log switch on or off during a recording adds a `logging enabled (pid N)` or `logging disabled` line to the file (see [Output sub-tab](#output-sub-tab))
- Times - The timestamps in log files and in `ui.log.txt` are your local time. Lines written by earlier versions stay in UTC
- Files are appended to. A file over 5 MB is emptied before the next write
- Log files can be read in the Log tab, without opening the folder (see [Output sub-tab](#output-sub-tab))

## Log tab

- Sub-tabs - **Engine** and **Output**, shown as buttons at the top of the tab
- Open Log button - Shown at the top right of the tab, on both sub-tabs. Opens `ui.log.txt` in the settings folder in your default app (see [Settings](#settings))
- Open Changelog button - Shown beside the Open Log button, on both sub-tabs. Opens the What's new window (see [Changelog](#changelog))
    - **Engine** - The messages described below
    - **Output** - The log files of the sites and streamers (see [Output sub-tab](#output-sub-tab))
- Messages - The most recent engine messages, with times in your local time. The number kept is the **Buffer Length** in the Log card on the Settings tab, 500 by default (see [Settings tab](#settings-tab)): 
- streamers going live
- recordings starting
- failed checks
- check problems appearing and clearing
- recordings stopping unexpectedly
- streamers switched on or off by their schedule
- recordings stopped for a streamer with a higher priority
- recording files that become `STALLED` or start growing again
- stalled recordings that are restarted, and restarts that reach the limit
- recorder output that matches an interrupt or error pattern, once for each kind per live session, with the pattern that matched
- recording resolutions that cannot be read, and ffprobe not found in PATH
- recordings restarted for a higher quality, and quality upgrades that are not armed or cannot work
- quick checks that are made, that reach the limit or that fail
- recording file paths that disagree or are not verified
- recording parts that start, and splits that are not seen growing in time
- recording files that are renamed to `_part1`, or cannot be renamed
- recording files, and other files, links and folders, that cannot be opened
- setup notices
- no cookies found in the browser
- unmatched live entries
- yt-dlp files made executable
- last live times found on Twitch
- Twitch lookups that fail
- notifications that fail to send, and `notify-send` not found on Linux
- test notifications sent from the Notifications page
- new posts that are found on an account
- posts whose download fails
- posts whose thumbnail or caption is missing, so an extra yt-dlp call is made
- posts whose automatic download is skipped for being over the size limit or the time limit
- accounts whose check fails or changes
- accounts whose ID lookup from a video URL fails
- posts whose files are deleted by auto-delete
- posts whose stream link cannot be found, or whose player cannot be started
- Files tab jobs that finish, fail or are cancelled
- update checks that fail
- updates downloading and ready to install
- updates that fail
- settings that are reset after an update
- broken config files that are replaced with **Start fresh**, and saves that are refused while saving is paused
- files that could not be written, such as a library or settings file, with the reason
- a previous session that ended without a clean exit (see [Crash log](#crash-log))
- jj-dlp being started again by the watcher after it exited unexpectedly, with how it ended (see [Watcher](#watcher))
- recorder processes left running by a previous session that are stopped (see [Recorder cleanup](#recorder-cleanup))
- `jj-dlp vX is up to date` after Check now
- History - Messages are saved to `ui.log.txt` in the settings folder with the full local date and time, and the Log tab shows the saved messages again after jj-dlp restarts (see [Settings](#settings))
    - Each launch adds a `--- session started (vX) ---` message, so the messages of different sessions can be told apart
    - When `ui.log.txt` grows past 4 MB it is cut back to its newest 5000 lines

### Output sub-tab

- Output sub-tab - The **Output** sub-tab of the Log tab. Lists the log files of every site and streamer, with a switch to turn each one on or off and a button to view it (see [Log files](#log-files))
    - List - Shown on the left, under a **LOG FILES** title, with the **stdout** and **stderr** column headers above it. The headers stay in view while the list scrolls
        - Each site is shown by name in its site color (see [Site color](#site-color)), with a **Checker** row, followed by a row for each of its streamers
        - Below the sites, a **POSTS** title is followed by each Posts site by name, with a **Downloader** row, then a `Checker per account` note and a row for each of its accounts, shown as `@<account>`. Each account has its own checker files, and the downloader files belong to the site
        - Each row has a switch and a **View** button under **stdout**, and another pair under **stderr**, with the **View** button right beside its switch. Each pair is one file, so the checker and every streamer have a stdout file and a stderr file
        - Long streamer names are shortened
        - The list follows the sites and streamers you have added, so a streamer that is added or removed on the Livestreams tab appears or disappears here. A site with no streamers only has its **Checker** row. The Posts rows follow the Posts sites and accounts in the same way
        - With no sites and no Posts sites, a message asks you to add one
    - Switches - The Posts switches are only on this sub-tab, and the Posts site settings have none. The checker and recorder switches are only on this sub-tab too, and the Site card and the streamer's settings have none
        - A switch is saved as soon as you click it, without **Save changes**
        - A Posts switch applies to the next check or download that starts, and does not change any other setting of the site
																																		   
        - Turning a recorder switch on or off during a recording adds a `logging enabled (pid N)` or `logging disabled` line to the file, from the next line of output (see [Log files](#log-files))
    - **View** button - Opens the file in the viewer on the right
        - Unavailable until the file exists. A file is only created when something is first written to it, so turning a switch on does not create the file until the next check, download or recording prints output
        - The button of the file that is open is highlighted
        - The list of files that exist is refreshed about every second
    - Viewer - Shown on the right, with the name of the open file above it. Only one file is open at a time, and opening another file replaces it
        - Opening a file shows its newest 500 lines, and then adds new lines as they are written, the same as `tail -f`
        - The oldest line is at the top and the newest at the bottom
        - Only complete lines are shown. A line that is still being written is shown once it is finished
        - The file is checked about once a second while the **Output** sub-tab is shown. Switching to another tab or to the **Engine** sub-tab stops the checks, and returning shows everything written in the meantime
        - The oldest lines are dropped as the viewer grows, so it does not slow down on a log that keeps growing
        - When a file is emptied for being over 5 MB, the viewer starts again from the new lines
        - When the open file does not exist yet, a message says it is waiting for it, and its lines are shown once it is created
        - The text can be selected and copied, with **Ctrl+C**, but not edited
        - **Follow** switch - Scrolls to the newest line as lines are added. Opening a file turns it on
            - Selecting text, moving the cursor with the keyboard and scrolling up turn it off, so the view stays still while you read or copy
            - While it is off, new lines are held back and the switch shows how many (example: `Follow (3 new)`). Turning it on adds them and scrolls to the newest line
        - **Wrap** switch - Wraps long lines. With it off, long lines stay on one line. It is saved, and applies to every file. It is separate from the **Wrap** switch on the Processes tab (see [Process panel](#process-panel-1))
        - **Copy** button - Copies the selected text, or the whole view when nothing is selected. Unavailable until a file is open
        - **Open logs folder** button - Shown beside **Copy**. Opens the `logs` folder inside the settings folder, and creates it if it does not exist
            - With a file open, the file is selected in the folder on Windows and macOS, and on Linux the folder is opened
            - Available while no file is open
    - The open file stays selected when you switch to another tab
- Settings - The **Wrap** switch is saved in `gui.json` (see [Settings](#settings))
