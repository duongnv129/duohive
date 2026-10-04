#!/usr/bin/env bash
# Re-sign the current branch's commits so GitHub marks them Verified.
#
#   scripts/resign-commits.sh                 re-sign commits not on origin/main
#   scripts/resign-commits.sh --base REF      re-sign commits after REF instead
#   scripts/resign-commits.sh --dry-run       list the commits and their status only
#   scripts/resign-commits.sh --push          force-push (with lease) when done
#
# Uses your git signing setup (user.signingkey, gpg.format). The key and the
# committer email must both be on your GitHub account for Verified to show.
# Commits are rewritten: their hashes change, author and dates are kept.
set -euo pipefail

base=""
dry_run=0
push=0

usage() {
    sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
}

die() {
    echo "error: $*" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --base) [ $# -ge 2 ] || { usage >&2; exit 2; }; base="$2"; shift 2 ;;
        --base=*) base="${1#--base=}"; shift ;;
        --dry-run) dry_run=1; shift ;;
        --push) push=1; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

git rev-parse --git-dir >/dev/null 2>&1 || die "not inside a git repository"
git_dir="$(git rev-parse --git-dir)"

branch="$(git symbolic-ref --quiet --short HEAD)" || die "HEAD is detached · check out a branch first"
case "$branch" in
    main | master) die "refusing to rewrite $branch · run this on a feature branch" ;;
esac

[ ! -d "$git_dir/rebase-merge" ] && [ ! -d "$git_dir/rebase-apply" ] ||
    die "a rebase is in progress · finish or abort it first"
if ! git diff --quiet || ! git diff --cached --quiet; then
    die "uncommitted changes · commit or stash them first"
fi

if [ -z "$base" ]; then
    git fetch --quiet origin main 2>/dev/null || echo "warning: couldn't fetch origin/main · using the local copy" >&2
    base="origin/main"
fi
git rev-parse --verify --quiet "$base^{commit}" >/dev/null || die "unknown base: $base"
fork_point="$(git merge-base "$base" HEAD)"

count="$(git rev-list --count "$fork_point..HEAD")"
[ "$count" -gt 0 ] || { echo "no commits on $branch after $base · nothing to do"; exit 0; }

# %G?: G good, U good but untrusted key, N none, B bad, E can't check, X/Y/R expired or revoked.
show_status() {
    git log --reverse --format='  %h  %G?  %an <%ae>  %s' "$fork_point..HEAD"
}

echo "$count commit(s) on $branch after $base (G = signed, N = not signed):"
show_status
[ "$dry_run" -eq 0 ] || exit 0

git config --get user.signingkey >/dev/null ||
    [ "$(git config --get gpg.format || true)" = "x509" ] ||
    die "no signing key · set one: git config user.signingkey <key-id>"

# Catch a broken signing setup (no agent, wrong key) before touching history.
if [ "$(git config --get gpg.format || echo openpgp)" = "openpgp" ]; then
    gpg_program="$(git config --get gpg.program || echo gpg)"
    echo "signing check" | "$gpg_program" --batch --yes --local-user "$(git config --get user.signingkey)" \
        --clearsign >/dev/null 2>&1 || die "gpg can't sign with user.signingkey · unlock or fix the key first"
fi

committer_email="$(git config --get user.email || true)"
if git log --format='%ae' "$fork_point..HEAD" | grep -qvxF "$committer_email"; then
    echo "note: some commits have another author email; GitHub verifies the signer's own (committer) email" >&2
fi

backup="backup/$branch-$(date +%Y%m%d%H%M%S)"
git branch "$backup"
echo "backup branch: $backup"

# --force-rebase rewrites every commit even when the base is unchanged, so each
# gets a fresh signature; --committer-date-is-author-date keeps the dates.
echo "re-signing…"
if ! git rebase --quiet --force-rebase --gpg-sign --rebase-merges \
    --committer-date-is-author-date "$fork_point"; then
    git rebase --abort 2>/dev/null || true
    die "rebase failed and was aborted · $branch is unchanged (backup: $backup)"
fi

echo "result:"
show_status
unsigned="$(git log --format='%G?' "$fork_point..HEAD" | grep -cvE '^[GU]$' || true)"
if [ "$unsigned" -gt 0 ]; then
    echo "warning: $unsigned commit(s) still don't show a good signature locally" >&2
fi

if [ "$push" -eq 1 ]; then
    git push --force-with-lease origin "$branch"
    echo "pushed $branch"
else
    echo "done · push with: git push --force-with-lease origin $branch"
fi
echo "undo with: git reset --hard $backup"
