# Main-menu background

`background.html` is shown full-window behind the main menu, in a native webview.

- Replace the file with your own. It must be **one self-contained file**: embed all scripts,
  styles, fonts and images (inline or `data:` URLs). Navigation to `http(s)` is blocked.
- The game injects the menu into your page (a shadow-DOM layer above everything, so your CSS
  cannot restyle it). You only draw the background.
- The page can react to menu navigation:
  `window.addEventListener('bts-menu-page', e => e.detail.page /* main|new|load|settings */)`.
- The page is kept at the window size; use `100vw`/`100vh`. Keep it light - it renders whenever
  the main menu is open (the 3D scene is paused meanwhile).
- File lookup: `assets/menu/background.html` in the working directory, beside the executable,
  or in the project root.
- Requires Linux/X11 with WebKitGTK. Otherwise (Wayland, load error, page not answering within
  5 s) the game falls back to its built-in 3D menu.
- Build without it: `cargo build -p game --no-default-features`.
