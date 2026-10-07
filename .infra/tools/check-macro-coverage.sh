#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <cargo-llvm-cov-json>" >&2
    exit 2
fi

coverage_json=$1
if [ ! -f "$coverage_json" ]; then
    echo "macro coverage report does not exist: $coverage_json" >&2
    exit 2
fi

summary=$(jq -er '
  if (.data | type) != "array" or (.data | length) == 0 then
    error("coverage report has no data")
  else
    reduce .data[] as $item (
      {functions:{covered:0,count:0},lines:{covered:0,count:0},regions:{covered:0,count:0}};
      .functions.covered += ($item.totals.functions.covered // 0)
      | .functions.count += ($item.totals.functions.count // 0)
      | .lines.covered += ($item.totals.lines.covered // 0)
      | .lines.count += ($item.totals.lines.count // 0)
      | .regions.covered += ($item.totals.regions.covered // 0)
      | .regions.count += ($item.totals.regions.count // 0)
    )
  end
' "$coverage_json")

for metric in functions lines regions; do
    covered=$(jq -er ".${metric}.covered" <<<"$summary")
    count=$(jq -er ".${metric}.count" <<<"$summary")
    case "$metric" in
        functions) threshold=90 ;;
        lines) threshold=85 ;;
        regions) threshold=85 ;;
    esac
    if [ "$count" -le 0 ]; then
        echo "macro $metric coverage has an empty denominator" >&2
        exit 1
    fi
    percent=$(awk -v covered="$covered" -v count="$count" 'BEGIN { printf "%.2f", covered * 100 / count }')
    printf 'macro %-9s %s%% (%s/%s), minimum %s%%\n' "$metric" "$percent" "$covered" "$count" "$threshold"
    if ! awk -v covered="$covered" -v count="$count" -v threshold="$threshold" \
        'BEGIN { exit (covered * 100 >= count * threshold) ? 0 : 1 }'; then
        echo "macro $metric coverage is below its baseline-derived minimum" >&2
        exit 1
    fi
done
