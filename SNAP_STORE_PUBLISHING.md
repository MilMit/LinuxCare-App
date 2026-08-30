# LinuxCare on Ubuntu App Center / Snap Store

LinuxCare is distributed to Ubuntu App Center through the public Snap Store.

## Channel policy

- Development and beta tags (`v*-alpha*`, `v*-beta*`, `v*-rc*`) publish to the Snap Store `beta` channel.
- Stable tags publish to the `stable` channel once `snapcraft.yaml` uses `grade: stable`.
- Pull requests build, locally install, connect the local Polkit interface, validate D-Bus activation, and GUI-smoke-test the snap without publishing it.

## Security model in the Snap

The Snap keeps LinuxCare's existing privilege boundary:

- the GTK/Libadwaita GUI runs as the desktop user;
- the privileged helper is a system D-Bus service using the existing `net.milmit.LinuxCare.Helper` name;
- the helper is D-Bus activated rather than started as a permanent boot service;
- the official Snap `polkit` interface installs LinuxCare's existing action policy and the helper continues to authorize each sensitive operation through Polkit;
- no generic privileged shell or arbitrary privileged command API is exposed.

## First-time publisher setup

LinuxCare intentionally uses `confinement: classic` because its core purpose is host-level maintenance and diagnostics. Public distribution therefore requires Store review/approval for:

1. classic confinement;
2. installation of the `net.milmit.LinuxCare.Helper` system D-Bus name;
3. auto-connection of the `polkit` plug with action prefix `net.milmit.LinuxCare` so App Center users do not need a manual post-install connection.

The detailed technical rationale is in `SNAP_CLASSIC_REVIEW.md`.

## Publisher setup

1. Install Snapcraft and authenticate with the Canonical account that will own LinuxCare.
2. Register the `linuxcare` snap name in the Snap Store.
3. Build/upload the first revision and submit the Store review requests listed above.
4. After approval, export narrowly-scoped Store credentials for CI:

   ```sh
   snapcraft export-login --snaps=linuxcare \
     --acls package_access,package_push,package_update,package_release \
     exported.txt
   ```

5. Add the contents of `exported.txt` to the GitHub repository secret named `SNAPCRAFT_STORE_CREDENTIALS`.

After that, tagged releases are built and published automatically by `.github/workflows/snap-store.yml`.

## App Center

Once the snap has passed Store review and a revision is released to a public channel, it becomes discoverable through the Snap Store ecosystem used by Ubuntu App Center. Beta releases remain on the `beta` channel until a stable LinuxCare release is promoted.
