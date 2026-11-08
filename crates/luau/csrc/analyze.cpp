// luau-analyze's checker over files held in memory (the `analyze` feature,
// native only): Luau's Analysis library as its CLI (CLI/src/Analyze.cpp)
// uses it, with the API's definitions loaded first (the CLI has no flag for
// them; S0's patch added one) and a project's `require("name")` resolved to
// its `game/name.lua`. docs/STUDIO-2.md §4.13, Decided 33: the old solver,
// nonstrict mode, type errors only (no lints); reported, never blocking.

#include "Luau/BuiltinDefinitions.h"
#include "Luau/Config.h"
#include "Luau/ConfigResolver.h"
#include "Luau/Error.h"
#include "Luau/FileResolver.h"
#include "Luau/Frontend.h"
#include "Luau/ToString.h"

#include <cstdlib>
#include <cstring>
#include <map>
#include <string>

namespace
{

struct MemoryFiles : Luau::FileResolver
{
    std::map<std::string, std::string> files;

    std::optional<Luau::SourceCode> readSource(const Luau::ModuleName& name) override
    {
        auto it = files.find(name);
        if (it == files.end())
            return std::nullopt;
        return Luau::SourceCode{it->second, Luau::SourceCode::Module};
    }

    // require("enemies") is game/enemies.lua, as the engine's require has it.
    std::optional<Luau::ModuleInfo> resolveModule(const Luau::ModuleInfo* context, Luau::AstExpr* node, const Luau::TypeCheckLimits& limits) override
    {
        if (Luau::AstExprConstantString* expr = node->as<Luau::AstExprConstantString>())
        {
            std::string path = "game/" + std::string(expr->value.data, expr->value.size) + ".lua";
            if (files.count(path))
                return Luau::ModuleInfo{path};
        }
        return std::nullopt;
    }

    std::string getHumanReadableModuleName(const Luau::ModuleName& name) const override
    {
        return name;
    }
};

void line(std::string& out, const std::string& name, const Luau::Location& loc, const std::string& message)
{
    out += name + "\t" + std::to_string(loc.begin.line + 1) + "\t" + std::to_string(loc.begin.column + 1) + "\t";
    for (char c : message)
        out += (c == '\n' || c == '\t') ? ' ' : c;
    out += "\n";
}

char* copy(const std::string& s)
{
    char* p = static_cast<char*>(malloc(s.size() + 1));
    memcpy(p, s.c_str(), s.size() + 1);
    return p;
}

} // namespace

// Checks `n` files (names and sources) against `ndefs` definition files.
// Returns one problem a line, "<file>\t<line>\t<column>\t<message>" (empty
// for none), malloc'd for the caller to free with sv_analyze_free. A
// definition file that doesn't load is reported against its own name.
extern "C" char* sv_analyze(
    const char* const* def_names,
    const char* const* defs,
    const size_t* def_lens,
    int ndefs,
    const char* const* names,
    const char* const* sources,
    const size_t* lens,
    int n
)
{
    std::string out;
    try
    {
        MemoryFiles files;
        for (int i = 0; i < n; i++)
            files.files[names[i]] = std::string(sources[i], lens[i]);
        Luau::NullConfigResolver config;
        config.defaultConfig.mode = Luau::Mode::Nonstrict;
        Luau::FrontendOptions options;
        options.runLintChecks = false;
        Luau::Frontend frontend(Luau::SolverMode::Old, &files, &config, options);
        Luau::registerBuiltinGlobals(frontend, frontend.globals);
        for (int i = 0; i < ndefs; i++)
        {
            std::string_view src(defs[i], def_lens[i]);
            Luau::LoadDefinitionFileResult r =
                frontend.loadDefinitionFile(frontend.globals, frontend.globals.globalScope, src, std::string("@") + def_names[i], false);
            if (!r.success)
            {
                for (const Luau::ParseError& e : r.parseResult.errors)
                    line(out, def_names[i], e.getLocation(), e.getMessage());
                if (r.module)
                    for (const Luau::TypeError& e : r.module->errors)
                        line(out, def_names[i], e.location, Luau::toString(e));
                return copy(out);
            }
        }
        Luau::freeze(frontend.globals.globalTypes);
        for (int i = 0; i < n; i++)
        {
            Luau::CheckResult cr = frontend.check(names[i]);
            for (const Luau::TypeError& e : cr.errors)
            {
                if (const Luau::SyntaxError* s = Luau::get_if<Luau::SyntaxError>(&e.data))
                    line(out, e.moduleName, e.location, s->message);
                else
                    line(out, e.moduleName, e.location, Luau::toString(e, Luau::TypeErrorToStringOptions{frontend.fileResolver}));
            }
        }
    }
    catch (const std::exception& e)
    {
        out += std::string("-\t0\t0\tthe checker failed: ") + e.what() + "\n";
    }
    return copy(out);
}

extern "C" void sv_analyze_free(char* p)
{
    free(p);
}
