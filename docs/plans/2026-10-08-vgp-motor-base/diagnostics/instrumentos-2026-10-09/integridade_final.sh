SP=/tmp/claude-1000/-home-gabrielgadea-projects-analise/e7c33fba-4d8d-4026-84db-fe7083ece971/scratchpad
B=$HOME/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts
V=$HOME/.claude/touring/vgp
echo '== limites CP (unilateral 95%): 7/1200 5/600 2/600 3/1200 1/600 0/600'
python3 -I $SP/limites.py 7 1200 5 600 2 600 3 1200 1 600 0 600
echo '== vivo intocado?'
( cd $V/corpus_v0d && find . -type f -print0 | sort -z | xargs -0 sha256sum ) > $SP/vivo_agora.txt
cmp -s $SP/vivo_agora.txt $SP/vivo_manifest_antes.txt && echo "IDENTICO ao manifesto de antes ($(wc -l < $SP/vivo_manifest_antes.txt) arquivos)" || echo DIFERE
echo '== bundle'
for f in corpus_v0d.py corpus_v5_inline_neg.py test_corpus_v0d.py shell_inline.py; do echo "$f $(sha256sum $B/$f | cut -c1-16) $(stat -c %y $B/$f | cut -c12-19)"; done
echo '== holdouts'
for h in corpus_holdout corpus_holdout_disjunto; do echo "$h: $(ls $V/$h | tr '\n' ' ')"; done
