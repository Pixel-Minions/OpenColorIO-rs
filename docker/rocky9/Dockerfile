# Linux reference platform (PLAN.md D11): Rocky Linux 9, glibc 2.34, VFX Reference Platform CY2027.
# OCIO calls glibc's libm at runtime, so the oracle wheel and the port must run on this image
# for Linux pixel checks to be meaningful.
FROM rockylinux/rockylinux:9.8

RUN dnf install -y --setopt=install_weak_deps=False \
        gcc gcc-c++ make git which findutils diffutils tar xz procps-ng \
    && dnf clean all

ENV RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    UV_INSTALL_DIR=/opt/uv/bin \
    UV_PYTHON_INSTALL_DIR=/opt/uv/python \
    UV_CACHE_DIR=/opt/uv/cache \
    PATH=/opt/cargo/bin:/opt/uv/bin:$PATH

# The same toolchain as rust-toolchain.toml.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.98.1 -c rustfmt,clippy \
    && chmod -R a+w /opt/cargo /opt/rustup

RUN curl -LsSf https://astral.sh/uv/0.11.7/install.sh | sh \
    && uv python install 3.13 \
    && chmod -R a+rwX /opt/uv

# The checkout is mounted here; build output goes to a Linux-only target directory.
WORKDIR /work
ENV CARGO_TARGET_DIR=/work/target/rocky9

# Phase 7, P7-2/P7-4 (owner, 2026-10-25): upstream's GLSL 1.20 harness on
# llvmpipe, plus GLSL compile checks. Keep this last so the existing compiler,
# Python and uv layers are reused when this image is rebuilt locally.
# Exact NVRs from Rocky 9 AppStream (2026-10-25); do not silently upgrade the
# renderer that defines the GPU suite's baseline.
RUN dnf install -y --setopt=install_weak_deps=False \
        mesa-dri-drivers-25.2.7-5.el9_8.x86_64 \
        mesa-filesystem-25.2.7-5.el9_8.x86_64 \
        mesa-libEGL-25.2.7-5.el9_8.x86_64 \
        mesa-libGL-25.2.7-5.el9_8.x86_64 \
        mesa-libgbm-25.2.7-5.el9_8.x86_64 \
        libglvnd-1.3.4-1.el9.x86_64 \
        libglvnd-egl-1.3.4-1.el9.x86_64 \
        libglvnd-glx-1.3.4-1.el9.x86_64 \
        libglvnd-opengl-1.3.4-1.el9.x86_64 \
        glslang-16.0.0-1.el9.x86_64 \
    && dnf clean all

ENV EGL_PLATFORM=surfaceless \
    LIBGL_ALWAYS_SOFTWARE=true \
    GALLIUM_DRIVER=llvmpipe
