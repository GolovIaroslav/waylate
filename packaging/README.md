# Packaging

The root `PKGBUILD` is meant for local Arch builds from a checkout:

```bash
makepkg -si
```

Before publishing to AUR, update `url`, add a real source archive, regenerate
checksums, and test in a clean chroot.
