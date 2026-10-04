#!/usr/bin/env bash
# Re-sign every commit on the current branch and force-push it, so GitHub
# marks them Verified.
#
#   scripts/resign-commits.sh                 re-sign all commits, then force-push
#   scripts/resign-commits.sh --base REF      re-sign only the commits after REF
#   scripts/resign-commits.sh --no-push       re-sign locally, don't push
#   scripts/resign-commits.sh --dry-run       list the commits and their status only
#   scripts/resign-commits.sh --yes           don't ask before rewriting
#
# Uses your git signing setup (user.signingkey, gpg.format). The key and the
# committer email must both be on your GitHub account for Verified to show.
# Commits are rewritten: hashes change, authors and dates are kept, and
# anyone else with the branch must reset to the new one.
set -euo pipefail

base=""
dry_run=0
push=1
assume_yes=0

usage() {
    sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
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
        --no-push) push=0; shift ;;
        -y | --yes) assume_yes=1; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
done

git rev-parse --git-dir >/dev/null 2>&1 || die "not inside a git repository"
git_dir="$(git rev-parse --git-dir)"

branch="$(git symbolic-ref --quiet --short HEAD)" || die "HEAD is detached · check out a branch first"

if [ -d "$git_dir/rebase-merge" ] || [ -d "$git_dir/rebase-apply" ]; then
    die "a rebase is in progress · finish or abort it first"
fi
if ! git diff --quiet || ! git diff --cached --quiet; then
    die "uncommitted changes · commit or stash them first"
fi

# Push to the branch's upstream when it has one, else origin/<branch>.
remote="$(git config --get "branch.$branch.remote" || echo origin)"
remote_branch="$(git config --get "branch.$branch.merge" || echo "refs/heads/$branch")"
remote_branch="${remote_branch#refs/heads/}"

if [ -n "$base" ]; then
    git rev-parse --verify --quiet "$base^{commit}" >/dev/null || die "unknown base: $base"
    fork_point="$(git merge-base "$base" HEAD)" || die "$base shares no history with $branch"
    range="$fork_point..HEAD"
    rebase_onto=("$fork_point")
    scope="after $base"
else
    range="HEAD"
    rebase_onto=(--root)
    scope="since the first commit"
fi

count="$(git rev-list --count "$range")"
[ "$count" -gt 0 ] || { echo "no commits on $branch $scope · nothing to do"; exit 0; }

# %G?: G good, U good but untrusted key, N none, B bad, E can't check, X/Y/R expired or revoked.
show_status() {
    git log --reverse --format='  %h  %G?  %an <%ae>  %s' "$range"
}

echo "$count commit(s) on $branch $scope (G = signed, N = not signed):"
show_status
[ "$dry_run" -eq 0 ] || exit 0

signing_format="$(git config --get gpg.format || echo openpgp)"
signing_key="$(git config --get user.signingkey || true)"
if [ -z "$signing_key" ] && [ "$signing_format" != "x509" ]; then
    die "no signing key · set one: git config user.signingkey <key-id>"
fi

# Catch a broken signing setup (no agent, locked or wrong key) before touching history.
if [ "$signing_format" = "openpgp" ]; then
    gpg_program="$(git config --get gpg.program || echo gpg)"
    echo "signing check" | "$gpg_program" --batch --yes --local-user "$signing_key" \
        --clearsign >/dev/null 2>&1 || die "gpg can't sign with user.signingkey · unlock or fix the key first"
fi

committer_email="$(git config --get user.email || true)"
[ -n "$committer_email" ] || die "user.email is not set"
echo "commits will be signed and committed as <$committer_email>; this email must be verified on GitHub"

if [ "$assume_yes" -eq 0 ]; then
    action="rewrite $count commit(s) on $branch"
    [ "$push" -eq 0 ] || action="$action and force-push to $remote/$remote_branch"
    [ -t 0 ] || die "not a terminal · pass --yes to $action"
    printf '%s? [y/N] ' "$action"
    read -r answer
    case "$answer" in
        y | Y | yes | YES) ;;
        *) echo "cancelled · nothing changed"; exit 0 ;;
    esac
fi

backup="backup/$branch-$(date +%Y%m%d%H%M%S)"
git branch "$backup"
echo "backup branch: $backup"

# --force-rebase rewrites every commit even when nothing moves, so each gets a
# fresh signature; --committer-date-is-author-date keeps the dates;
# --rebase-merges keeps merge commits.
echo "re-signing…"
if ! git rebase --quiet --force-rebase --gpg-sign --rebase-merges \
    --committer-date-is-author-date "${rebase_onto[@]}"; then
    git rebase --abort 2>/dev/null || true
    die "rebase failed and was aborted · $branch is unchanged (backup: $backup)"
fi

if ! git diff --quiet "$backup" HEAD; then
    die "re-signed tree differs from the original · restore with: git reset --hard $backup"
fi

echo "result:"
show_status
bad="$(git log --format='%G?' "$range" | grep -cvE '^[GU]$' || true)"
[ "$bad" -eq 0 ] || die "$bad commit(s) don't show a good signature · nothing pushed (undo: git reset --hard $backup)"

if [ "$push" -eq 1 ]; then
    echo "force-pushing to ${remote}/${remote_branch}…"
    git push --force-with-lease "$remote" "HEAD:refs/heads/$remote_branch" ||
        die "push refused (branch protection or newer commits on $remote) · local branch is re-signed (undo: git reset --hard $backup)"
    echo "pushed $branch"
else
    echo "done · push with: git push --force-with-lease $remote HEAD:$remote_branch"
fi
echo "undo with: git reset --hard $backup"
