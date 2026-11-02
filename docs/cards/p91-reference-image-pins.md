# P91: preserve the renderer-qualified Rocky reference image

The release audit found that the Rocky 9.8 tag and llvm-libs were floating even
though W0013-W0018 require the recorded Mesa/LLVM renderer. Pin the base image to
the OCI index digest used in the passing CI and the exact installed LLVM NEVRA:

- rockylinux/rockylinux:9.8 at
  sha256:8101994123cf3d0a8fee517bee7f39e555c7d92bd2d9eb3303cc988a0eeed00f;
- llvm-libs-21.1.8-2.el9.x86_64, alongside the existing Mesa 25.2.7-5.el9_8 pins.

The published index was rechecked and agrees with the passing PR98 image build.
The existing local reference image also reports that exact LLVM package. This
keeps the established llvmpipe (LLVM 21.1.8, 256 bits), Mesa 25.2.7 baseline; it
does not broaden any renderer condition or waiver.

Owned files: docker/rocky9/Dockerfile, docker/rocky9/reference.repo and this card.
Both DNF transactions use only a dedicated repository directory and fixed 9.8
URLs. Each repository has the live Rocky tree followed by its official vault
location. DNF's ordered baseurl fallback survives the minor release moving out
of pub/rocky; it does not fall forward to another minor version. Keep RPM signature
checks and fail when a repository is unavailable. The original enabled BaseOS,
AppStream and Extras repositories remain available; no CRB repository is enabled.
An installed-package check also rejects an unexpected LLVM release/architecture.

References: [DNF baseurl semantics](https://dnf.readthedocs.io/en/latest/conf_ref.html#baseurl)
and [Rocky repository/vault policy](https://github.com/rocky-linux/wiki.rockylinux.org/blob/main/docs/rocky/repo.md).
The normal Linux CI builds the image from the pinned base and runs the existing
EGL probe and unchanged GPU suite. Retain local identity/configuration and fallback
probe evidence in kept/continuation/2026-11-01/. No renderer thresholds, waiver
files, library implementation, GPU compile checks, release path or notices change.

Local validation: Rocky's real DNF parser accepts all three fixed repository
pairs with signatures required and unavailable repositories fatal. An offline
DNF metadata probe successfully falls back from a missing first location to a
valid second location, and rejects both locations missing. HTTP checks find the
three live 9.8 metadata files and exact LLVM RPM. The existing image's EGL probe
reports the recorded renderer/version above. These probes use ephemeral capped
containers; the existing image/tag is not rebuilt or changed locally. Fresh image
builds and all unchanged GPU checks remain required in the grouped normal CI.
