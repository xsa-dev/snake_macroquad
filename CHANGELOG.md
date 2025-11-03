# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Fullscreen mode**: Game now launches in fullscreen mode by default
- **Sound settings system**: Added new settings screen (press S) with ability to adjust sound effects volume
- **Map preview in lobby**: Main menu displays a preview of the map with animated snake, demonstrating current difficulty settings
- **Arrow key navigation**: Added ability to navigate menu using up/down arrow keys
- **Sound settings persistence**: Sound volume is now saved to `snake_save.json` and restored on next launch
- **Enhanced controls**: Added Q key support to quit the game from all screens

### Changed
- **Improved lobby UI**: All text in main menu is now centered and adaptively scales to screen size
- **Improved Game Over UI**: Game over screen also uses adaptive text centering
- **Dynamic scaling**: All UI elements now use `screen_width()` and `screen_height()` instead of fixed constants for better multi-resolution support
- **Sound optimization**: All sound effects now respect user's volume setting

### Technical
- Added `volume` field to `SnakeGame` struct for sound volume management
- Added `sound_volume` field to `SaveData` struct for settings persistence
- Added new `Settings(SettingsState)` screen variant to `Screen` enum
- Added `selected`, `preview_map`, `preview_pos`, `preview_dir`, `preview_last_move` fields to `LobbyState` for map preview support
- Modified `window_conf()` function to configure fullscreen mode and high DPI
- Improved screen state management system using `next_screen` for smoother transitions
