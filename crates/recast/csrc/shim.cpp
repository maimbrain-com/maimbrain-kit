// A small C API over Recast 1.6.0 (the bake), Detour (path queries) and
// DetourCrowd (agents), for src/lib.rs. From spike S10's harness (tag
// spike/s10-navigation-final). Tiled, as Recast's Sample_TileMesh builds:
// each tile from the triangles that touch it with a border (src/lib.rs picks
// them), then dtCreateNavMeshData, and addTile in its own call, so a tile
// can be built again on its own. Memory is the game's allocator (the
// patched RecastAlloc.cpp and DetourAlloc.cpp call mbrc_alloc).
//
// Recast and Detour are Copyright (c) 2009 Mikko Mononen, zlib licence
// (games/studio-engine2/NOTICES).
#include <math.h>
#include <stdint.h>
#include <string.h>

#include <new>

#include "DetourCommon.h"
#include "DetourCrowd.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshBuilder.h"
#include "DetourNavMeshQuery.h"
#include "Recast.h"

// A trap where nothing should get to (MSVC has no __builtin_trap).
#if defined(_MSC_VER) && !defined(__clang__)
#define MBRC_TRAP() abort()
#else
#define MBRC_TRAP() __builtin_trap()
#endif

// portable.h's replacements that are C++ (the maths ones are Rust's).
extern "C" void mbrc_qsort(void* base, size_t n, size_t size, int (*cmp)(const void*, const void*))
{
    // A stable insertion sort: the same order for equal keys on every libc.
    unsigned char* a = (unsigned char*)base;
    unsigned char tmp[64];
    if (size > sizeof(tmp))
        MBRC_TRAP();
    for (size_t i = 1; i < n; i++)
    {
        memcpy(tmp, a + i * size, size);
        size_t j = i;
        while (j > 0 && cmp(a + (j - 1) * size, tmp) > 0)
        {
            memcpy(a + j * size, a + (j - 1) * size, size);
            j--;
        }
        memcpy(a + j * size, tmp, size);
    }
}

// dtPolyQuery's vtable names it (its process() is pure). Weak, as libc++abi
// has one where Luau links it; both trap.
#if defined(__wasm__)
extern "C" __attribute__((weak)) void __cxa_pure_virtual()
{
    __builtin_trap();
}
#endif

extern "C" int mbrc_vsnprintf(char* buf, size_t n, const char*, va_list)
{
    if (n)
        buf[0] = 0;
    return 0;
}

struct MbrcCfg
{
    float cs, ch;
    float agentHeight, agentRadius, agentClimb, slopeDeg;
    int32_t regionMin, regionMerge;
    float edgeMaxLen, edgeMaxError;
    int32_t vertsPerPoly;
    float detailSampleDist, detailSampleMaxError;
    int32_t tileCells; // cells a tile's side, without the border
    float bmin[3], bmax[3];
    int32_t maxTiles;
};

struct MbrcAgent
{
    float radius, height, maxAccel, maxSpeed;
    int32_t avoidance; // Recast's demo's preset 0-3, or -1 for none
    float separation;
};

struct MbrcNav
{
    dtNavMesh* nav;
    dtNavMeshQuery* query;
    dtCrowd* crowd;
    dtQueryFilter filter;
    MbrcCfg cfg;
};

static int ilog2ceil(int v)
{
    int r = 0;
    while ((1 << r) < v)
        r++;
    return r;
}

extern "C" MbrcNav* mbrc_nav_new(const MbrcCfg* cfg)
{
    MbrcNav* s = (MbrcNav*)mbrc_alloc(sizeof(MbrcNav));
    if (!s)
        return 0;
    memset(s, 0, sizeof(MbrcNav));
    new (&s->filter) dtQueryFilter();
    s->cfg = *cfg;
    const float tcs = cfg->tileCells * cfg->cs;
    const int tileBits = rcMin(ilog2ceil(cfg->maxTiles < 1 ? 1 : cfg->maxTiles), 14);
    dtNavMeshParams params;
    memset(&params, 0, sizeof(params));
    rcVcopy(params.orig, cfg->bmin);
    params.tileWidth = tcs;
    params.tileHeight = tcs;
    params.maxTiles = 1 << tileBits;
    params.maxPolys = 1 << (22 - tileBits);
    s->nav = dtAllocNavMesh();
    s->query = dtAllocNavMeshQuery();
    if (!s->nav || !s->query || dtStatusFailed(s->nav->init(&params)) || dtStatusFailed(s->query->init(s->nav, 2048)))
    {
        dtFreeNavMeshQuery(s->query);
        dtFreeNavMesh(s->nav);
        mbrc_free(s);
        return 0;
    }
    return s;
}

extern "C" void mbrc_nav_free(MbrcNav* s)
{
    if (!s)
        return;
    if (s->crowd)
        dtFreeCrowd(s->crowd);
    dtFreeNavMeshQuery(s->query);
    dtFreeNavMesh(s->nav);
    mbrc_free(s);
}

extern "C" void mbrc_free_data(unsigned char* data)
{
    dtFree(data);
}

// One tile's navmesh data (dtAlloc'd) into *out, from the triangles given
// (world space), and its size; 0 when nothing there is walkable. stats:
// polygons, vertices, detail triangles. `vols` are solids whose insides
// aren't walkable (a closed trunk's floor would be, under its cap), as
// Recast's convex volumes: [n, ymin, ymax, x1, z1, … xn, zn] after each other,
// `nvol` floats in all.
extern "C" int32_t mbrc_build_tile(MbrcNav* s, const float* verts, int32_t nv, const int32_t* tris, int32_t nt, const float* vols, int32_t nvol, int32_t tx,
    int32_t ty, unsigned char** out, int32_t* stats)
{
    const MbrcCfg& c = s->cfg;
    *out = 0;
    stats[0] = stats[1] = stats[2] = 0;
    rcContext ctx(false);
    rcConfig cfg;
    memset(&cfg, 0, sizeof(cfg));
    cfg.cs = c.cs;
    cfg.ch = c.ch;
    cfg.walkableSlopeAngle = c.slopeDeg;
    cfg.walkableHeight = (int)ceilf(c.agentHeight / c.ch);
    cfg.walkableClimb = (int)floorf(c.agentClimb / c.ch);
    cfg.walkableRadius = (int)ceilf(c.agentRadius / c.cs);
    cfg.maxEdgeLen = (int)(c.edgeMaxLen / c.cs);
    cfg.maxSimplificationError = c.edgeMaxError;
    cfg.minRegionArea = c.regionMin * c.regionMin;
    cfg.mergeRegionArea = c.regionMerge * c.regionMerge;
    cfg.maxVertsPerPoly = c.vertsPerPoly;
    cfg.tileSize = c.tileCells;
    cfg.borderSize = cfg.walkableRadius + 3;
    cfg.width = cfg.tileSize + cfg.borderSize * 2;
    cfg.height = cfg.tileSize + cfg.borderSize * 2;
    cfg.detailSampleDist = c.detailSampleDist < 0.9f ? 0 : c.cs * c.detailSampleDist;
    cfg.detailSampleMaxError = c.ch * c.detailSampleMaxError;

    const float tcs = c.tileCells * c.cs;
    cfg.bmin[0] = c.bmin[0] + tx * tcs - cfg.borderSize * cfg.cs;
    cfg.bmin[1] = c.bmin[1];
    cfg.bmin[2] = c.bmin[2] + ty * tcs - cfg.borderSize * cfg.cs;
    cfg.bmax[0] = c.bmin[0] + (tx + 1) * tcs + cfg.borderSize * cfg.cs;
    cfg.bmax[1] = c.bmax[1];
    cfg.bmax[2] = c.bmin[2] + (ty + 1) * tcs + cfg.borderSize * cfg.cs;

    if (nt <= 0)
        return 0;
    unsigned char* areas = (unsigned char*)mbrc_alloc((size_t)nt);
    rcHeightfield* solid = rcAllocHeightfield();
    if (!areas || !solid || !rcCreateHeightfield(&ctx, *solid, cfg.width, cfg.height, cfg.bmin, cfg.bmax, cfg.cs, cfg.ch))
    {
        mbrc_free(areas);
        rcFreeHeightField(solid);
        return 0;
    }
    memset(areas, 0, (size_t)nt);
    rcMarkWalkableTriangles(&ctx, cfg.walkableSlopeAngle, verts, nv, tris, nt, areas);
    rcRasterizeTriangles(&ctx, verts, nv, tris, areas, nt, *solid, cfg.walkableClimb);
    mbrc_free(areas);

    rcFilterLowHangingWalkableObstacles(&ctx, cfg.walkableClimb, *solid);
    rcFilterLedgeSpans(&ctx, cfg.walkableHeight, cfg.walkableClimb, *solid);
    rcFilterWalkableLowHeightSpans(&ctx, cfg.walkableHeight, *solid);

    rcCompactHeightfield* chf = rcAllocCompactHeightfield();
    rcBuildCompactHeightfield(&ctx, cfg.walkableHeight, cfg.walkableClimb, *solid, *chf);
    rcFreeHeightField(solid);
    rcErodeWalkableArea(&ctx, cfg.walkableRadius, *chf);
    float poly[3 * 64];
    for (int i = 0; i + 3 <= nvol;)
    {
        const int n = (int)vols[i];
        const float y0 = vols[i + 1], y1 = vols[i + 2];
        if (n >= 3 && n <= 64 && i + 3 + 2 * n <= nvol)
        {
            for (int k = 0; k < n; k++)
            {
                poly[k * 3] = vols[i + 3 + 2 * k];
                poly[k * 3 + 1] = y0;
                poly[k * 3 + 2] = vols[i + 4 + 2 * k];
            }
            rcMarkConvexPolyArea(&ctx, poly, n, y0, y1, RC_NULL_AREA, *chf);
        }
        i += 3 + 2 * (n > 0 ? n : 0);
    }
    rcBuildDistanceField(&ctx, *chf);
    rcBuildRegions(&ctx, *chf, cfg.borderSize, cfg.minRegionArea, cfg.mergeRegionArea);

    rcContourSet* cset = rcAllocContourSet();
    rcBuildContours(&ctx, *chf, cfg.maxSimplificationError, cfg.maxEdgeLen, *cset);
    int size = 0;
    if (cset->nconts > 0)
    {
        rcPolyMesh* pmesh = rcAllocPolyMesh();
        rcBuildPolyMesh(&ctx, *cset, cfg.maxVertsPerPoly, *pmesh);
        rcPolyMeshDetail* dmesh = rcAllocPolyMeshDetail();
        rcBuildPolyMeshDetail(&ctx, *pmesh, *chf, cfg.detailSampleDist, cfg.detailSampleMaxError, *dmesh);
        for (int i = 0; i < pmesh->npolys; i++)
            pmesh->flags[i] = pmesh->areas[i] == RC_WALKABLE_AREA ? 1 : 0;
        if (pmesh->npolys > 0 && pmesh->nverts < 0xffff)
        {
            dtNavMeshCreateParams p;
            memset(&p, 0, sizeof(p));
            p.verts = pmesh->verts;
            p.vertCount = pmesh->nverts;
            p.polys = pmesh->polys;
            p.polyAreas = pmesh->areas;
            p.polyFlags = pmesh->flags;
            p.polyCount = pmesh->npolys;
            p.nvp = pmesh->nvp;
            p.detailMeshes = dmesh->meshes;
            p.detailVerts = dmesh->verts;
            p.detailVertsCount = dmesh->nverts;
            p.detailTris = dmesh->tris;
            p.detailTriCount = dmesh->ntris;
            p.walkableHeight = c.agentHeight;
            p.walkableRadius = c.agentRadius;
            p.walkableClimb = c.agentClimb;
            p.tileX = tx;
            p.tileY = ty;
            p.tileLayer = 0;
            rcVcopy(p.bmin, pmesh->bmin);
            rcVcopy(p.bmax, pmesh->bmax);
            p.cs = cfg.cs;
            p.ch = cfg.ch;
            p.buildBvTree = true;
            stats[0] = pmesh->npolys;
            stats[1] = pmesh->nverts;
            stats[2] = dmesh->ntris;
            if (!dtCreateNavMeshData(&p, out, &size))
            {
                *out = 0;
                size = 0;
            }
        }
        rcFreePolyMesh(pmesh);
        rcFreePolyMeshDetail(dmesh);
    }
    rcFreeContourSet(cset);
    rcFreeCompactHeightfield(chf);
    return size;
}

extern "C" void mbrc_remove_tile(MbrcNav* s, int32_t tx, int32_t ty)
{
    dtTileRef ref = s->nav->getTileRefAt(tx, ty, 0);
    if (ref)
        s->nav->removeTile(ref, 0, 0);
}

// Adds a tile's data (owned by the navmesh from here: it frees it), after
// taking out any tile already there. 1 when it went in (else the data is freed).
extern "C" int32_t mbrc_add_tile(MbrcNav* s, unsigned char* data, int32_t size, int32_t tx, int32_t ty)
{
    mbrc_remove_tile(s, tx, ty);
    if (dtStatusFailed(s->nav->addTile(data, size, DT_TILE_FREE_DATA, 0, 0)))
    {
        dtFree(data);
        return 0;
    }
    return 1;
}

// The nearest point on the navmesh to p within ext (half extents): 1 and
// the point in out, else 0.
extern "C" int32_t mbrc_nearest(MbrcNav* s, const float* p, const float* ext, float* out)
{
    dtPolyRef ref = 0;
    if (dtStatusFailed(s->query->findNearestPoly(p, ext, &s->filter, &ref, out)) || !ref)
        return 0;
    return 1;
}

// A straight path from a to b: corners (x, y, z) into out, at most max; the
// count, or -1 when either end isn't near the navmesh. *whole is 1 when it
// reaches b's polygon, 0 when it stops as near as it can get.
extern "C" int32_t mbrc_path(MbrcNav* s, const float* a, const float* b, const float* ext, float* out, int32_t max, int32_t* whole)
{
    dtPolyRef ra = 0, rb = 0;
    float na[3], nb[3];
    *whole = 0;
    if (dtStatusFailed(s->query->findNearestPoly(a, ext, &s->filter, &ra, na)) || !ra)
        return -1;
    if (dtStatusFailed(s->query->findNearestPoly(b, ext, &s->filter, &rb, nb)) || !rb)
        return -1;
    dtPolyRef polys[256];
    int np = 0;
    s->query->findPath(ra, rb, na, nb, &s->filter, polys, &np, 256);
    if (np == 0)
        return -1;
    float end[3];
    rcVcopy(end, nb);
    if (polys[np - 1] != rb)
        s->query->closestPointOnPoly(polys[np - 1], nb, end, 0);
    else
        *whole = 1;
    int n = 0;
    s->query->findStraightPath(na, end, polys, np, out, 0, 0, &n, max, 0);
    return n;
}

// Polygon edges for drawing, as segments (x1, y1, z1, x2, y2, z2) into out:
// the floats written (at most max).
extern "C" int32_t mbrc_outlines(const MbrcNav* s, float* out, int32_t max)
{
    const dtNavMesh* nav = s->nav;
    int w = 0;
    for (int i = 0; i < nav->getMaxTiles(); i++)
    {
        const dtMeshTile* t = nav->getTile(i);
        if (!t || !t->header)
            continue;
        for (int p = 0; p < t->header->polyCount; p++)
        {
            const dtPoly& poly = t->polys[p];
            for (int k = 0; k < poly.vertCount; k++)
            {
                if (w + 6 > max)
                    return w;
                const float* a = &t->verts[poly.verts[k] * 3];
                const float* b = &t->verts[poly.verts[(k + 1) % poly.vertCount] * 3];
                for (int j = 0; j < 3; j++)
                    out[w++] = a[j];
                for (int j = 0; j < 3; j++)
                    out[w++] = b[j];
            }
        }
    }
    return w;
}

extern "C" int32_t mbrc_crowd_init(MbrcNav* s, int32_t maxAgents, float maxRadius)
{
    if (s->crowd)
        dtFreeCrowd(s->crowd);
    s->crowd = dtAllocCrowd();
    if (!s->crowd || !s->crowd->init(maxAgents, maxRadius, s->nav))
        return 0;
    // Recast's demo's four avoidance presets (low, medium, good, high).
    dtObstacleAvoidanceParams p;
    memcpy(&p, s->crowd->getObstacleAvoidanceParams(0), sizeof(p));
    p.velBias = 0.5f;
    p.adaptiveDivs = 5;
    p.adaptiveRings = 2;
    p.adaptiveDepth = 1;
    s->crowd->setObstacleAvoidanceParams(0, &p);
    p.adaptiveDepth = 2;
    s->crowd->setObstacleAvoidanceParams(1, &p);
    p.adaptiveDivs = 7;
    p.adaptiveDepth = 3;
    s->crowd->setObstacleAvoidanceParams(2, &p);
    p.adaptiveRings = 3;
    s->crowd->setObstacleAvoidanceParams(3, &p);
    return 1;
}

static void agentParams(const MbrcAgent* a, dtCrowdAgentParams* ap)
{
    memset(ap, 0, sizeof(*ap));
    ap->radius = a->radius;
    ap->height = a->height;
    ap->maxAcceleration = a->maxAccel;
    ap->maxSpeed = a->maxSpeed;
    ap->collisionQueryRange = a->radius * 12.0f;
    ap->pathOptimizationRange = a->radius * 30.0f;
    ap->updateFlags = DT_CROWD_ANTICIPATE_TURNS | DT_CROWD_OPTIMIZE_VIS | DT_CROWD_OPTIMIZE_TOPO | (a->separation > 0 ? DT_CROWD_SEPARATION : 0)
        | (a->avoidance >= 0 ? DT_CROWD_OBSTACLE_AVOIDANCE : 0);
    ap->obstacleAvoidanceType = (unsigned char)(a->avoidance < 0 ? 0 : (a->avoidance > 3 ? 3 : a->avoidance));
    ap->separationWeight = a->separation;
}

extern "C" int32_t mbrc_crowd_add(MbrcNav* s, const float* pos, const MbrcAgent* a)
{
    dtCrowdAgentParams ap;
    agentParams(a, &ap);
    return s->crowd->addAgent(pos, &ap);
}

extern "C" void mbrc_crowd_params(MbrcNav* s, int32_t idx, const MbrcAgent* a)
{
    dtCrowdAgentParams ap;
    agentParams(a, &ap);
    s->crowd->updateAgentParameters(idx, &ap);
}

extern "C" void mbrc_crowd_remove(MbrcNav* s, int32_t idx)
{
    s->crowd->removeAgent(idx);
}

// Asks the agent to go to the navmesh's nearest point to pos within ext: 1
// when it's on its way.
extern "C" int32_t mbrc_crowd_target(MbrcNav* s, int32_t idx, const float* pos, const float* ext)
{
    dtPolyRef ref = 0;
    float nearest[3];
    s->crowd->getNavMeshQuery()->findNearestPoly(pos, ext, s->crowd->getFilter(0), &ref, nearest);
    if (!ref)
        return 0;
    return s->crowd->requestMoveTarget(idx, ref, nearest) ? 1 : 0;
}

extern "C" void mbrc_crowd_stop(MbrcNav* s, int32_t idx)
{
    s->crowd->resetMoveTarget(idx);
}

extern "C" void mbrc_crowd_update(MbrcNav* s, float dt)
{
    s->crowd->update(dt, 0);
}

// The agent: position (3), velocity (3), desired velocity (3), state,
// target state, corners, the next corner (3), whether its path is partial,
// the target (3), then its corners (3 each, up to 4).
extern "C" void mbrc_crowd_agent(MbrcNav* s, int32_t idx, float* out)
{
    const dtCrowdAgent* a = s->crowd->getAgent(idx);
    for (int i = 0; i < 3; i++)
    {
        out[i] = a->npos[i];
        out[3 + i] = a->vel[i];
        out[6 + i] = a->dvel[i];
        out[12 + i] = a->ncorners > 0 ? a->cornerVerts[i] : a->npos[i];
        out[16 + i] = a->targetPos[i];
    }
    out[9] = (float)a->state;
    out[10] = (float)a->targetState;
    out[11] = (float)a->ncorners;
    out[15] = a->partial ? 1.0f : 0.0f;
    for (int i = 0; i < DT_CROWDAGENT_MAX_CORNERS * 3; i++)
        out[19 + i] = i < a->ncorners * 3 ? a->cornerVerts[i] : 0.0f;
}

// Puts the agent where its body ended (Detour's corridor, so it stays on
// the navmesh), as Godot's NavigationAgent3D follows its parent.
extern "C" void mbrc_crowd_place(MbrcNav* s, int32_t idx, const float* pos)
{
    dtCrowdAgent* a = s->crowd->getEditableAgent(idx);
    if (!a || !a->active || a->state != DT_CROWDAGENT_STATE_WALKING)
        return;
    a->corridor.movePosition(pos, s->query, s->crowd->getFilter(a->params.queryFilterType));
    dtVcopy(a->npos, a->corridor.getPos());
}
