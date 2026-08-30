# LinuxCare on Ubuntu App Center / Snap Store

LinuxCare is distributed to Ubuntu App Center through the public Snap Store.

## Channel policy

- Development and beta tags (`v*-alpha*`, `v*-beta*`, `v*-rc*`) publish to the Snap Store `beta` channel.
- Stable tags publish to the `stable` channel once `snapcraft.yaml` uses `grade: stable`.
- Pull requests build, locally install, and smoke-test the snap without publishing it.

## First-time publisher setup

LinuxCare intentionally uses `confinement: classic` because its core purpose is host-level maintenance and diagnostics. The Snap Store must approve classic confinement before the snap can be publicly released.

1. Install Snapcraft and authenticate with the Canonical account that will own LinuxCare.
2. Register the `linuxcare` snap name in the Snap Store.
3. Build/upload the first revision and complete the Store review required for classic confinement.
4. Export narrowly-scoped Store credentials for CI:

   ```sh
   snapcraft export-login --snaps=linuxcare \
     --acls package_access,package_push,package_update,package_release \
     exported.txt
   ```

5. Add the contents of `exported.txt` to the GitHub repository secret named `SNAPCRAFT_STORE_CREDENTIALS`.

After that, tagged releases are built and published automatically by `.github/workflows/snap-store.yml`.

## App Center

Once the snap has passed Store review and a revision is released to a public channel, it becomes discoverable through the Snap Store ecosystem used by Ubuntu App Center. Beta releases remain on the beta channel until a stable LinuxCare release is promoted.
