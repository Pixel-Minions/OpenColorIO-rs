# Diagnostic only. Instrument the captured nan_bug shader (work/style_aces2_nan_bug/full.glsl,
# the exact text llvmpipe compiled in CI 40950573272) so that it writes intermediates to a
# global array. Output pixel x (row 0) = D[x]; row 1 = OCIOMain's normal result, which must
# stay bit-identical to the captured GPU image (checked by run_nan_bug.sh).
# Optional: --inject NAME=hexbits ... replaces one GPU intermediate with a given value
# (used for substitution experiments: the value comes from the wheel CPU).
import re, sys

src, dst = sys.argv[1], sys.argv[2]
inject = dict(a.split("=", 1) for a in sys.argv[3:])
uniforms = []
s = open(src).read()

def sub(old, new, count=1):
    global s
    n = s.count(old)
    assert n == count, (old, n)
    s = s.replace(old, new)

def inj(name, expr):
    """Return GLSL that optionally overrides variable 'expr' named NAME."""
    if name not in inject:
        return ""
    n = len(inject[name].split(","))
    uniforms.append("uniform vec4 INJ_%s;\n" % name)
    return " %s = INJ_%s.%s;" % (expr, name, "xyzw"[:n])

# Globals.
sub("// Declaration of all helper methods\n",
    "// Declaration of all helper methods\nvec4 D[32];\nint gcCall = 0;\n")

# Remap internals (indices 12 + 6*call ...).
sub("""  float boundary_ratio = gamut_boundary_M / reach_boundary_M;
  float proportion = max(boundary_ratio, 0.75);
  float threshold = proportion * gamut_boundary_M;
""", """  float boundary_ratio = gamut_boundary_M / reach_boundary_M;
  float proportion = max(boundary_ratio, 0.75);
  float threshold = proportion * gamut_boundary_M;
  D[12 + 6 * gcCall + 3] = vec4(boundary_ratio, proportion, threshold, M);
""")
sub("""  float scale = reach_offset / ((reach_offset / gamut_offset) - 1.0f);
  float nd = m_offset / scale;
""", """  float scale = reach_offset / ((reach_offset / gamut_offset) - 1.0f);
  float nd = m_offset / scale;
  D[12 + 6 * gcCall + 4] = vec4(m_offset, gamut_offset, reach_offset, scale);
  D[12 + 6 * gcCall + 5] = vec4(nd, reach_offset / gamut_offset, nd - 1.0f, 0.0);
""")

# gamut_compress internals.
sub("""    float J_intersect_source = ocio_solve_J_intersect1(JMh.r, JMh.g, focusJ, slope_gain);
""", """    float J_intersect_source = ocio_solve_J_intersect1(JMh.r, JMh.g, focusJ, slope_gain);""" + inj("JIS", "J_intersect_source") + """
""")
sub("""    float gamutBoundaryM = ocio_find_gamut_boundary_intersection1(JMcusp, gamma_top_inv, gamma_bottom_inv, J_intersect_source, J_intersect_cusp, gamut_slope);
""", """    float gamutBoundaryM = ocio_find_gamut_boundary_intersection1(JMcusp, gamma_top_inv, gamma_bottom_inv, J_intersect_source, J_intersect_cusp, gamut_slope);
    D[12 + 6 * gcCall + 0] = vec4(Jx, focusJ, slope_gain, J_intersect_source);
    D[12 + 6 * gcCall + 1] = vec4(gamut_slope, J_intersect_cusp, gamutBoundaryM, gamma_top_inv);
""")
sub("""    float remapped_M = ocio_remap_M_inv1(M, gamutBoundaryM, reachBoundaryM);
    float remapped_J = J_intersect_source + remapped_M * gamut_slope;
""", """    float remapped_M = ocio_remap_M_inv1(M, gamutBoundaryM, reachBoundaryM);
    float remapped_J = J_intersect_source + remapped_M * gamut_slope;
    D[12 + 6 * gcCall + 2] = vec4(reachBoundaryM, remapped_M, remapped_J, h);
    gcCall = gcCall + 1;
""")

# Main pipeline stages.
sub("""    vec4 res = vec4(myPixel.rgb.r, myPixel.rgb.g, myPixel.rgb.b, myPixel.a);
    res = vec4(1., 1.04163837, 1.12397468, 1.) * res;""", """   """ + inj("LUT", "myPixel.rgb") + """
    D[0] = myPixel;
    vec4 res = vec4(myPixel.rgb.r, myPixel.rgb.g, myPixel.rgb.b, myPixel.a);
    res = vec4(1., 1.04163837, 1.12397468, 1.) * res;""")
sub("""        vec3 F_L_v = pow(abs(lms), vec3(0.419999987, 0.419999987, 0.419999987));
""", """       """ + inj("LMS", "lms") + """
        vec3 F_L_v = pow(abs(lms), vec3(0.419999987, 0.419999987, 0.419999987));
""")
sub("""        vec3 rgb_a = (sign(lms) * F_L_v) / ( 27.1299992 + F_L_v);
        Aab = mat3(20.25881, 15480., 1720., 10.129405, -16887.2734, 1720., 0.506470263, 1407.27271, -3440.) * rgb_a.rgb;
""", """        vec3 rgb_a = (sign(lms) * F_L_v) / ( 27.1299992 + F_L_v); D[29] = vec4(F_L_v, 0.0);""" + inj("RGBA", "rgb_a") + """
        Aab = mat3(20.25881, 15480., 1720., 10.129405, -16887.2734, 1720., 0.506470263, 1407.27271, -3440.) * rgb_a.rgb;
        D[1] = vec4(myPixel.rgb, 0.0); D[2] = vec4(lms, 0.0); D[3] = vec4(rgb_a, 0.0);""" + inj("AAB", "Aab") + """ D[4] = vec4(Aab, 0.0);
""")
sub("""      myPixel.rgb = JMh;
    }
    float h_rad = myPixel.b * 0.0174532924;""", """      myPixel.rgb = JMh;""" + inj("JMH", "myPixel.rgb") + """
      D[5] = vec4(myPixel.rgb, 0.0);
    }
    float h_rad = myPixel.b * 0.0174532924;""")
sub("""    float reachMaxM = ocio_reach_m_table_1_sample(myPixel.b);
""", """    float reachMaxM = ocio_reach_m_table_1_sample(myPixel.b);
    D[6] = vec4(h_rad, cos_hr, sin_hr, reachMaxM);
""")
sub("""      vec3 JMGcusp = ocio_gamut_cusp_table_1_sample(myPixel.b);
""", """      vec3 JMGcusp = ocio_gamut_cusp_table_1_sample(myPixel.b);
      D[7] = vec4(JMGcusp, mix(JMGcusp.r, 511.809357, 0.300000));
""")
sub("""      myPixel.rgb = unCompressedJMh;
    }
""", """      myPixel.rgb = unCompressedJMh;""" + inj("TJMH", "myPixel.rgb") + """
      D[8] = vec4(myPixel.rgb, 0.0);
    }
""")
sub("""      myPixel.rgb = vec3(J, M, h);
    }
""", """      myPixel.rgb = vec3(J, M, h);
      D[9] = vec4(myPixel.rgb, 0.0);
    }
""")
sub("""        vec3 lms = sign(rgb_a) * pow( 27.1299992 * rgb_a_lim / (1.0f - rgb_a_lim), vec3(2.38095236, 2.38095236, 2.38095236));
""", """        vec3 lms = sign(rgb_a) * pow( 27.1299992 * rgb_a_lim / (1.0f - rgb_a_lim), vec3(2.38095236, 2.38095236, 2.38095236));
        D[10] = vec4(Aab, 0.0); D[11] = vec4(rgb_a, 0.0); D[30] = vec4(lms, 0.0);
""")
sub("""      myPixel.rgb = JMh;
    }
  }
""", """      myPixel.rgb = JMh;
      D[31] = vec4(myPixel.rgb, 0.0);
    }
  }
""")

# main(): row 0 -> D[x]; row 1 -> normal output.
sub("""    vec4 col = texture2D(img, gl_TexCoord[0].st);
    gl_FragColor = OCIOMain(col);""", """    vec4 col = texture2D(img, gl_TexCoord[0].st);
    vec4 outc = OCIOMain(col);
    int x = int(gl_FragCoord.x);
    gl_FragColor = (gl_FragCoord.y < 1.0) ? D[x] : outc;""")
# Injected values are runtime uniforms (set by probe from PROBE_UNIFORMS) so nothing is constant-folded.
sub("vec4 D[32];\n", "vec4 D[32];\n" + "".join(uniforms))
print(";".join("INJ_%s:%s" % (k, v) for k, v in inject.items()))
open(dst, "w").write(s)
