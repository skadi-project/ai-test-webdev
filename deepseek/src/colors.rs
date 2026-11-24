//! Brand colors for programming languages (GitHub Linguist palette subset).

/// Returns the canonical color for a language, or a neutral gray.
pub fn color_for(language: &str) -> &'static str {
    match language {
        "Rust" => "#dea584",
        "Python" => "#3572A5",
        "JavaScript" => "#f1e05a",
        "TypeScript" => "#3178c6",
        "Go" => "#00ADD8",
        "C" => "#555555",
        "C++" => "#f34b7d",
        "C#" => "#178600",
        "Java" => "#b07219",
        "Ruby" => "#701516",
        "PHP" => "#4F5D95",
        "Swift" => "#F05138",
        "Kotlin" => "#A97BFF",
        "Scala" => "#c22d40",
        "Haskell" => "#5e5086",
        "Elixir" => "#6e4a7e",
        "Erlang" => "#B83998",
        "Clojure" => "#db5855",
        "Lua" => "#000080",
        "Perl" => "#0298c3",
        "R" => "#198CE7",
        "Dart" => "#00B4AB",
        "Zig" => "#ec915c",
        "Nim" => "#ffc200",
        "Shell" => "#89e051",
        "PowerShell" => "#012456",
        "HTML" => "#e34c26",
        "CSS" => "#563d7c",
        "SCSS" => "#c6538c",
        "Sass" => "#a53b70",
        "Vue" => "#41b883",
        "Svelte" => "#ff3e00",
        "Astro" => "#ff5a03",
        "Nix" => "#7e7eff",
        "Dockerfile" => "#384d54",
        "Makefile" => "#427819",
        "HCL" => "#844FBA",
        "PLpgSQL" => "#336790",
        "SQL" => "#e38c00",
        "Jupyter Notebook" => "#DA5B0B",
        "MDX" => "#fcb32c",
        "Markdown" => "#083fa1",
        "Objective-C" => "#438eff",
        "Assembly" => "#6E4C13",
        "WebAssembly" => "#04133b",
        "Vim Script" => "#199f4b",
        "Emacs Lisp" => "#c065db",
        "OCaml" => "#ef7a08",
        "F#" => "#b845fc",
        "Groovy" => "#4298b8",
        "Julia" => "#a270ba",
        "Crystal" => "#000100",
        "V" => "#4f87c4",
        "Solidity" => "#AA6746",
        "TeX" => "#3D6117",
        "GLSL" => "#5686a5",
        "HLSL" => "#aace60",
        "GDScript" => "#355570",
        "Racket" => "#3c5caa",
        "Fortran" => "#4d41b1",
        "COBOL" => "#2e2e2e",
        "Visual Basic .NET" => "#945db7",
        "Batchfile" => "#C1F12E",
        "Jsonnet" => "#0064bd",
        "Terraform" => "#844FBA",
        _ => "#8b949e",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_languages_have_distinct_colors() {
        assert_eq!(color_for("Rust"), "#dea584");
        assert_eq!(color_for("TypeScript"), "#3178c6");
        assert_ne!(color_for("Rust"), color_for("Python"));
    }

    #[test]
    fn unknown_language_falls_back() {
        assert_eq!(color_for("Whitespace"), "#8b949e");
    }
}
