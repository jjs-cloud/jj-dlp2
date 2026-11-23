## 2.0.
- Added this changelog popup after each update
- yt-dlp checker calls are now batched for streamers with the same "Check every", instead of a separate yt-dlp call for each streamer. This only applies if you have set the "Check every" override value in streamer settings. Site-level yt-dlp calls checker are always batched.
- Added "Youtube Videos" in the Posts tab. Defaults to 1 video downloaded at a time, max 1000 MB, and max 120 seconds download time. These setttings can be changed in Settings > Posts > Youtube Videos.
- Added the ability to remove failed posts (via the x button in the top right corner of the thumbnail)
- The x button in the top right corner of the thumbnail on unsaved posts now changes color on hover.
- Removed the time picker (in streamer settings) in favor of a simpler time selector.
