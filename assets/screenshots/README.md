# LinuxCare AppStream screenshots

Only real screenshots from a verified LinuxCare build belong here. Do not substitute mockups or generated UI artwork for AppStream screenshots.

Capture at least these three views from the Beta UI:

1. `01-dashboard.png` — Dashboard / Health Score / Recommended Actions.
2. `02-safety-cleaner.png` — Smart Cleaner or Safety & Timeline showing Quarantine/Undo semantics without personal filenames.
3. `03-doctors.png` — one representative diagnostics view such as Hardware Doctor, Network Doctor, or Process Intelligence.

Requirements used by the LinuxCare release validator:

- PNG format.
- 16:9 aspect ratio.
- Width at least 1280 px for release assets (stricter than the AppStream minimum).
- No usernames, hostnames, IP addresses, serial numbers, file paths, tokens, or unrelated personal data.
- Use the actual current Beta build, not a design mockup.

After capture:

```bash
python3 scripts/validate-screenshots.py assets/screenshots
```

The public URLs can be added to the `<screenshots>` section of `data/net.milmit.LinuxCare.metainfo.xml` once the repository/site location for release media is final.
