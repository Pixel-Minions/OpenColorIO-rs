// Diagnostic only (claude/aces2-gpu-debug): fp64 emulations of GLSL builtins, rounded once to
// float. Validated against glibc by native/check_acc.py.
double acc_log2d(double x) {
  int e; double m = frexp(x, e);
  if (m < 0.70710678118654752LF) { m = m * 2.0LF; e = e - 1; }
  double z = (m - 1.0LF) / (m + 1.0LF); double z2 = z * z; double s = 0.0LF; double p = z;
  for (int k = 0; k < 16; ++k) { s = s + p / double(2 * k + 1); p = p * z2; }
  return double(e) + 2.0LF * s * 1.4426950408889634074LF;
}
double acc_expd(double f) {  // |f| <= ~0.35
  double t = 1.0LF; double s = 1.0LF;
  for (int k = 1; k < 22; ++k) { t = t * f / double(k); s = s + t; }
  return s;
}
double acc_exp2d(double y) { double n = floor(y + 0.5LF); return ldexp(acc_expd((y - n) * 0.69314718055994530942LF), int(n)); }
float acc_pow(float x, float y) { if (!(x > 0.0)) return pow(x, y); return float(acc_exp2d(double(y) * acc_log2d(double(x)))); }
vec3 acc_pow(vec3 x, vec3 y) { return vec3(acc_pow(x.x, y.x), acc_pow(x.y, y.y), acc_pow(x.z, y.z)); }
vec4 acc_pow(vec4 x, vec4 y) { return vec4(acc_pow(x.x, y.x), acc_pow(x.y, y.y), acc_pow(x.z, y.z), acc_pow(x.w, y.w)); }
float acc_log(float x) { if (!(x > 0.0)) return log(x); return float(acc_log2d(double(x)) * 0.69314718055994530942LF); }
float acc_log2(float x) { if (!(x > 0.0)) return log2(x); return float(acc_log2d(double(x))); }
double acc_atand(double x) {  // any finite x
  bool inv = abs(x) > 1.0LF; double t = inv ? 1.0LF / x : x;
  for (int i = 0; i < 4; ++i) t = t / (1.0LF + sqrt(1.0LF + t * t));   // 16 * atan(t) == atan(orig)
  double t2 = t * t; double s = 0.0LF; double p = t;
  for (int k = 0; k < 12; ++k) { s = s + ((k % 2 == 0) ? p : -p) / double(2 * k + 1); p = p * t2; }
  s = s * 16.0LF;
  const double HPI = 1.57079632679489661923LF;
  return inv ? ((x > 0.0LF) ? HPI - s : -HPI - s) : s;
}
float acc_atan(float y, float x) {
  const double PI = 3.14159265358979323846LF;
  double dy = double(y), dx = double(x);
  if (dx > 0.0LF) return float(acc_atand(dy / dx));
  if (dx < 0.0LF) return float(acc_atand(dy / dx) + ((dy >= 0.0LF) ? PI : -PI));
  return atan(y, x);
}
double acc_sind(double a) {  // |a| <= ~7
  const double TWO_PI = 6.28318530717958647692LF;
  a = a - TWO_PI * floor(a / TWO_PI + 0.5LF);
  double t = a; double s = a; double a2 = a * a;
  for (int k = 1; k < 16; ++k) { t = -t * a2 / double((2 * k) * (2 * k + 1)); s = s + t; }
  return s;
}
float acc_sin(float a) { return float(acc_sind(double(a))); }
float acc_cos(float a) { return float(acc_sind(double(a) + 1.57079632679489661923LF)); }
