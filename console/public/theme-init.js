// The theme chosen in the console (light or dark), applied before the first
// paint. Without a choice, brand/tokens.css follows prefers-color-scheme.
try {
  var theme = localStorage.getItem('stretto-console:theme')
  if (theme === 'light' || theme === 'dark')
    document.documentElement.setAttribute('data-theme', theme)
} catch {
  // Storage can be unavailable (private windows, blocked site data): follow the system.
}
