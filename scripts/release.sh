#!/usr/bin/env bash
# 使い方: scripts/release.sh <lm.zst> <model.zst>
#
# 署名タグを push し、Release にモデルを載せ、公開された物が nix/models.nix の hash と
# 合うと確かめてから main を push する。main を最後に push するのは、bot が input を
# 上げたときに、モデルが取れない commit を見せないため。
set -euo pipefail

remote=${REMOTE:-origin}

if [ $# -ne 2 ]; then
  echo "usage: $0 <lm.zst> <model.zst>" >&2
  exit 2
fi
lm=$1
model=$2

cd "$(dirname "$0")/.."

version=$(nix eval --impure --raw --expr \
  '(builtins.fromTOML (builtins.readFile ./sekken-rs/Cargo.toml)).workspace.package.version')
tag=v$version

# 検証。ここまでは何も書き換えない。
[ "$(git branch --show-current)" = main ] || { echo "main で実行する" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "作業ツリーが汚れている" >&2; exit 1; }
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "$tag は既にある" >&2
  exit 1
fi

# models.nix は関数なので評価せず、`"<file>" = "<hash>";` の行から読む。
for entry in "lm.zst:$lm" "model.zst:$model"; do
  name=${entry%%:*}
  path=${entry#*:}
  want=$(sed -n "s|^ *\"$name\" = \"\\(sha256-[^\"]*\\)\";|\\1|p" nix/models.nix)
  got=$(nix hash file --sri "$path")
  if [ "$want" != "$got" ]; then
    echo "$name の hash が nix/models.nix と違う: models.nix=$want 実物=$got" >&2
    exit 1
  fi
done

# 公開。
git tag -s "$tag" -m "$tag"
git push "$remote" "$tag"
gh release create "$tag" --verify-tag --title "$tag" --notes "" \
  "$lm" "$model" THIRD-PARTY.md

# 公開された URL から取れて hash が合うか。
nix build ".#sekken-models" --no-link

git push "$remote" main
