# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.

"""Assert the Phase 7 software-renderer prerequisites, without extra packages.

The context setup follows HeadlessApp (src/libutils/oglapphelpers/oglapp.cpp:
309-362 @ v2.5.2). This infrastructure probe uses only Python's standard library;
the test harness itself is the Rust port in ocio-gpu-exec.
"""

import ctypes as c


def main():
    egl = c.CDLL("libEGL.so.1")
    ptr = c.c_void_p
    integer = c.c_int
    functions = {
        "eglGetDisplay": (ptr, [ptr]),
        "eglInitialize": (c.c_uint, [ptr, ptr, ptr]),
        "eglChooseConfig": (c.c_uint, [ptr, ptr, ptr, integer, ptr]),
        "eglCreatePbufferSurface": (ptr, [ptr, ptr, ptr]),
        "eglBindAPI": (c.c_uint, [c.c_uint]),
        "eglCreateContext": (ptr, [ptr, ptr, ptr, ptr]),
        "eglMakeCurrent": (c.c_uint, [ptr, ptr, ptr, ptr]),
        "eglGetProcAddress": (ptr, [c.c_char_p]),
        "eglDestroyContext": (c.c_uint, [ptr, ptr]),
        "eglDestroySurface": (c.c_uint, [ptr, ptr]),
        "eglTerminate": (c.c_uint, [ptr]),
    }
    for name, (result, arguments) in functions.items():
        function = getattr(egl, name)
        function.restype = result
        function.argtypes = arguments

    display = egl.eglGetDisplay(None)
    assert display, "EGL could not be initialized."
    major, minor = integer(), integer()
    assert egl.eglInitialize(display, c.byref(major), c.byref(minor)), (
        "EGL display connection couldn't be started."
    )
    surface = context = None
    try:
        # EGL_SURFACE_TYPE/PBUFFER_BIT, BLUE/GREEN/RED/DEPTH_SIZE,
        # EGL_RENDERABLE_TYPE/OPENGL_BIT and EGL_NONE, as upstream's HeadlessApp.
        attributes = (integer * 13)(
            0x3033, 1, 0x3022, 8, 0x3023, 8, 0x3024, 8,
            0x3025, 8, 0x3040, 8, 0x3038,
        )
        config, count = ptr(), integer()
        assert egl.eglChooseConfig(display, attributes, c.byref(config), 1, c.byref(count))
        assert count.value == 1, "No EGL configuration for desktop OpenGL."
        pbuffer = (integer * 5)(0x3057, 16, 0x3056, 16, 0x3038)
        surface = egl.eglCreatePbufferSurface(display, config, pbuffer)
        assert surface, "Could not create EGL pbuffer."
        assert egl.eglBindAPI(0x30A2), "Could not select desktop OpenGL."
        context = egl.eglCreateContext(display, config, None, None)
        assert context, "Could not create EGL context."
        assert egl.eglMakeCurrent(display, surface, surface, context), (
            "Could not make EGL context current."
        )
        address = egl.eglGetProcAddress(b"glGetString")
        assert address, "OpenGL glGetString is unavailable."
        get_string = c.CFUNCTYPE(c.c_char_p, c.c_uint)(address)
        info = {
            label: get_string(token).decode("ascii")
            for label, token in (
                ("GL Vendor", 0x1F00), ("GL Renderer", 0x1F01),
                ("GL Version", 0x1F02), ("GLSL Version", 0x8B8C),
            )
        }
        for label, value in info.items():
            print(f"{label}: {value}")
        print(f"EGL Version: {major.value}.{minor.value}")
        assert "llvmpipe" in info["GL Renderer"], "GPU suite requires llvmpipe."
        assert "Compatibility Profile" in info["GL Version"], (
            "GPU suite requires OpenGL's fixed-function compatibility pipeline."
        )
        gl_version = tuple(map(int, info["GL Version"].split()[0].split(".")[:2]))
        glsl_version = tuple(map(int, info["GLSL Version"].split()[0].split(".")[:2]))
        assert gl_version >= (2, 0), "OpenGL 2.0 not supported."
        assert glsl_version >= (1, 20), "GLSL 1.20 not supported."
    finally:
        egl.eglMakeCurrent(display, None, None, None)
        if context:
            egl.eglDestroyContext(display, context)
        if surface:
            egl.eglDestroySurface(display, surface)
        egl.eglTerminate(display)


if __name__ == "__main__":
    main()
