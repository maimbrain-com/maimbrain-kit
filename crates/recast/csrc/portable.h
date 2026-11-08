// Included before every Recast, Detour and DetourCrowd file (-include), so
// the same code runs natively and in wasm, bit for bit (spike S10):
//
// - cosf, sinf, atan2f: Rust's `libm` crate (src/lib.rs exports them), not
//   the platform's libm, which differs natively (Apple's, glibc's) from
//   wasm's. Recast takes one cosf (the walkable slope), DetourCrowd's
//   obstacle avoidance sin, cos and atan2 every agent, every frame. And
//   roundf (Detour's tile coordinates), which wasm has no instruction for.
// - qsort: a stable insertion sort (shim.cpp). qsort isn't stable, so equal
//   keys (Recast sorts contour holes and diagonals) come out in another
//   order with another libc: Apple's natively baked another navmesh.
// - vsnprintf: rcContext's log; a game has nowhere to print it, and it
//   keeps printf out of the wasm.
// - mbrc_alloc, mbrc_free: the game's allocator (src/lib.rs), which the
//   patched RecastAlloc.cpp and DetourAlloc.cpp call (build.rs).
#pragma once
// The headers that declare what's renamed below come first, with the
// real names: glibc's math.h builds names by pasting tokens (its SIMD
// declarations), and libstdc++'s <cstdlib> and <cmath> say `using ::qsort`
// and `using ::roundf`, both of which a rename made earlier breaks. Their
// include guards then keep Recast's own includes from declaring them again.
#include <stddef.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#ifdef __cplusplus
#include <cmath>
#include <cstdio>
#include <cstdlib>
#endif
#ifdef __cplusplus
extern "C" {
#endif
float mbrc_cosf(float x);
float mbrc_sinf(float x);
float mbrc_atan2f(float y, float x);
float mbrc_roundf(float x);
void mbrc_qsort(void* base, size_t n, size_t size, int (*cmp)(const void*, const void*));
int mbrc_vsnprintf(char* buf, size_t n, const char* fmt, va_list args);
void* mbrc_alloc(size_t size);
void mbrc_free(void* ptr);
#ifdef __cplusplus
}
#endif
#define cosf mbrc_cosf
#define sinf mbrc_sinf
#define atan2f mbrc_atan2f
#define roundf mbrc_roundf
#define qsort mbrc_qsort
#define vsnprintf mbrc_vsnprintf
