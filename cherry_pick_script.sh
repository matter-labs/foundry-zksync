#!/bin/bash
set -e

# Get the list of commits to cherry-pick (in chronological order)
COMMITS=($(git log --oneline upstream/master 3e32767d2f..ffdc91bc5e --reverse | awk '{print $1}'))

# Start from where we left off (we already did the first 3)
START_INDEX=3
TOTAL=${#COMMITS[@]}

echo "Cherry-picking commits ${START_INDEX}+1 to $TOTAL out of $TOTAL total commits"

for ((i=$START_INDEX; i<$TOTAL; i++)); do
    COMMIT=${COMMITS[i]}
    COMMIT_MSG=$(git log --oneline -1 $COMMIT | cut -d' ' -f2-)
    
    echo "[$((i+1))/$TOTAL] Cherry-picking $COMMIT: $COMMIT_MSG"
    
    if git cherry-pick $COMMIT; then
        echo "  ✓ Clean cherry-pick"
    else
        echo "  ! Conflicts detected, resolving..."
        
        # Resolve conflicts by keeping our version
        git checkout --ours . 2>/dev/null || true
        
        # Handle deleted files
        git status --porcelain | grep "^DU " | cut -c4- | while read file; do
            echo "    Removing deleted file: $file"
            git rm "$file" 2>/dev/null || true
        done
        
        # Stage all changes
        git add .
        
        # Check if there are changes to commit
        if git diff --cached --quiet; then
            echo "    Empty commit, allowing"
            git commit --allow-empty
        else
            echo "    Committing resolved conflicts"
            git cherry-pick --continue
        fi
    fi
done

echo "All commits cherry-picked!"