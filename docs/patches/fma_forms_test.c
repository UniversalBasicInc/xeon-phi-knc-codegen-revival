// Correctness test for the KNC backend's FMA forms: accumulate chains in 8-wide double vectors,
// compared lane by lane against scalar fma(). Exit 0 = all 8*N lanes bit-identical.
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <string.h>
typedef double v8 __attribute__((vector_size(64)));
#define N 64
__attribute__((noinline)) void chain(const v8 *k, const v8 *c, v8 *acc, v8 hh, v8 *out, int n) {
    for (int i = 0; i < n; i++) {
        v8 a = acc[i];
        // three accumulate FMAs where k and c stay live (forces the copy without the 231 form)
        a = __builtin_elementwise_fma(k[i], c[0], a);
        a = __builtin_elementwise_fma(k[(i + 1) % n], c[1], a);
        a = __builtin_elementwise_fma(k[(i + 2) % n], c[2], a);
        out[i] = __builtin_elementwise_fma(hh, a, acc[i]);
        acc[i] = a;   // k, c, acc all still live after each fma
    }
}
int main(void) {
    static v8 k[N], c[3], acc[N], out[N], acc0[N];
    unsigned long long s = 0x9E3779B97F4A7C15ull;
    double *kd = (double *)k, *cd = (double *)c, *ad = (double *)acc;
    for (int i = 0; i < N * 8; i++) { s ^= s << 13; s ^= s >> 7; s ^= s << 17; kd[i] = (double)(s >> 11) / 9007199254740992.0 * 2.0 - 1.0; }
    for (int i = 0; i < 24; i++) { s ^= s << 13; s ^= s >> 7; s ^= s << 17; cd[i] = (double)(s >> 11) / 9007199254740992.0 * 4.0 - 2.0; }
    for (int i = 0; i < N * 8; i++) { s ^= s << 13; s ^= s >> 7; s ^= s << 17; ad[i] = (double)(s >> 11) / 9007199254740992.0 * 10.0; }
    memcpy(acc0, acc, sizeof acc);
    v8 hh; for (int l = 0; l < 8; l++) hh[l] = 0.001 * (l + 1);
    chain(k, c, acc, hh, out, N);
    int bad = 0;
    for (int i = 0; i < N; i++) for (int l = 0; l < 8; l++) {
        double a = acc0[i][l];
        a = fma(k[i][l], c[0][l], a);
        a = fma(k[(i + 1) % N][l], c[1][l], a);
        a = fma(k[(i + 2) % N][l], c[2][l], a);
        double o = fma(hh[l], a, acc0[i][l]);
        double got_o = out[i][l], got_a = acc[i][l];
        if (memcmp(&o, &got_o, 8) != 0 || memcmp(&a, &got_a, 8) != 0) { if (bad < 5) printf("mismatch i=%d l=%d: %.17g vs %.17g\n", i, l, o, got_o); bad++; }
    }
    printf("%s: %d lane mismatches of %d\n", bad ? "FAIL" : "OK", bad, N * 8);
    return bad != 0;
}
