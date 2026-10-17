# 配布する学習済みモデル。hash はバージョンを上げるコミットで、Release に上げる実物から
# `nix hash file --sri` で書く。
{
  fetchurl,
  linkFarm,
}: let
  # エンジンと同じ Release のモデルを使うので、版は Cargo.toml の 1 箇所から読む。
  version = (builtins.fromTOML (builtins.readFile ../sekken-rs/Cargo.toml)).workspace.package.version;
  hashes = {
    "lm.zst" = "sha256-+EmWlqqAKcwi/YB0hM7LDbo3E8uleOMynZhqZCDjwV4=";
    "model.zst" = "sha256-hKrkXlsMz19ZBUUrvoQCSu9l+5XbUmECttCpiCtSHMI=";
  };
  # name に版を入れないと、hash を書き忘れても旧版を持つ store で取り直さずに通る。
  fetch = file: hash:
    fetchurl {
      name = "sekken-${version}-${file}";
      url = "https://github.com/Warashi/sekken/releases/download/v${version}/${file}";
      inherit hash;
    };
in
  linkFarm "sekken-models-${version}" (
    builtins.attrValues (builtins.mapAttrs (file: hash: {
        name = file;
        path = fetch file hash;
      })
      hashes)
  )
