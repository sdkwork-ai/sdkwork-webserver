<?php

declare(strict_types=1);

namespace SDKWork\Webserver\BackendSdk\Models;

use SDKWork\Webserver\BackendSdk\Models\MetricsMetricTotals;
use SDKWork\Webserver\BackendSdk\Models\MetricsSeries;
use SDKWork\Webserver\BackendSdk\Models\MetricsSeriesWindow;
use SDKWork\Webserver\BackendSdk\Models\MetricsWindowBounds;

/**
 * The dashboard's cardinal metrics, each reported across the same four **half-open** windows (`today`, `last_7_days`, `current_month`, `lifetime`). Three groups share the window vocabulary but not its meaning: for the entity counts a window names when the subjects arrived, and `lifetime` is therefore the live population rather than an arrival count; for the metered traffic every window is a volume accumulated inside it, and `lifetime` is everything the facts cover — which is why `trafficSince` reports the day they start on; for the storage figures a narrow window is the part of the current holding added inside it, and `lifetime` is what is held *now* — a held figure rather than a running total, since an object removed from storage stops counting in every window including the one it was born in.
`series` reports the same entity metrics at **per-day** granularity, over the window named by `seriesWindow` rather than over the four above: the four card windows belong to the product and are resolved server-side from the clock, while the series window belongs to the caller and comes from the query string, because the plot it feeds also draws the traffic reading's per-day series and that plot has one x domain. A caller that draws both must pass one pair of bounds to both operations rather than omitting them: each operation resolves an omitted bound from the clock on its own, and two defaults are two windows. The two are reported separately so a surface cannot label a day's figure with a period it was not cut against.
 */
final class MetricsSummaryResponse
{
    /** The UTC day the windows were resolved against. Reported because the windows are relative to a clock, and a surface labelling "today" from its own date while the figures were cut against this one would name a window the numbers do not cover. */
    public ?string $asOf = null;

    /** Whether the figures cover every tenant rather than the caller's own. Reported so a surface cannot render a platform-wide count as if it were the caller's own, or the reverse. */
    public ?bool $platformScope = null;

    /** Earliest UTC day the metered traffic facts cover. The basis of the lifetime traffic figures: without it, "lifetime" would read as the life of the product rather than the life of the fact table. */
    public ?string $trafficSince = null;

    public array $windows = [];

    /** The entity counts. The platform reading reports users, tenants, applications, and agents; the own-tenant reading reports users, applications, and agents, because a tenant counting itself is always one. A metric this deployment cannot read at all is **absent** here and named in `unassembledMetrics` instead of being reported as `0`. */
    public array $entities = [];

    public array $traffic = [];

    /** The occupied storage, as `storage.used_bytes` (`BYTE`) and `storage.object_count` (`COUNT`). Sourced from sdkwork-drive's own occupancy predicate, so the figure here and the figure Drive refuses an upload against are the same number. Reported by both reaches: unlike the tenant count, a tenant's own consumption is answerable for that tenant. */
    public array $storage = [];

    /** The per-day entity arrivals over `seriesWindow`, one entry per metric, each with its days ascending. Separate from `entities` rather than a fifth window on them: the cards' windows are the server's, this one's is the caller's, and folding them together would give one row two meanings for "the window".
Only the entities are reported here. The metered volumes have their own per-day shape in the traffic readings, and storage has none because a day's occupancy is not reconstructable: a deleted object leaves the catalog, and its absence is indistinguishable from never having existed. */
    public array $series = [];

    public ?MetricsSeriesWindow $seriesWindow = null;

    /** Metric ids this deployment could not read, because the source behind them is not assembled here. Not metrics that came back empty: a metric answered `0` is reported as the figure `0`, and this list is the other case, where no figure exists and none was invented. The agents figure is the one that can land here — no topology profile declares the agents module, so a deployment that does not run it cannot count agents, and saying so is the only true answer.
A surface must draw these as a missing capability rather than as a zero, which is the same distinction the whole reading draws with a `503`. */
    public array $unassembledMetrics = [];

    public function __construct(array $data = [])
    {
        $this->asOf = array_key_exists('asOf', $data)
            ? $data['asOf']
            : null;
        $this->platformScope = array_key_exists('platformScope', $data)
            ? $data['platformScope']
            : null;
        $this->trafficSince = array_key_exists('trafficSince', $data)
            ? $data['trafficSince']
            : null;
        $this->windows = array_key_exists('windows', $data)
            ? is_array($data['windows'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsWindowBounds::fromArray($item) : $item, $data['windows']))
                : []
            : [];
        $this->entities = array_key_exists('entities', $data)
            ? is_array($data['entities'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsMetricTotals::fromArray($item) : $item, $data['entities']))
                : []
            : [];
        $this->traffic = array_key_exists('traffic', $data)
            ? is_array($data['traffic'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsMetricTotals::fromArray($item) : $item, $data['traffic']))
                : []
            : [];
        $this->storage = array_key_exists('storage', $data)
            ? is_array($data['storage'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsMetricTotals::fromArray($item) : $item, $data['storage']))
                : []
            : [];
        $this->series = array_key_exists('series', $data)
            ? is_array($data['series'])
                ? array_values(array_map(static fn($item) => is_array($item) ? MetricsSeries::fromArray($item) : $item, $data['series']))
                : []
            : [];
        $this->seriesWindow = array_key_exists('seriesWindow', $data)
            ? is_array($data['seriesWindow']) ? MetricsSeriesWindow::fromArray($data['seriesWindow']) : null
            : null;
        $this->unassembledMetrics = array_key_exists('unassembledMetrics', $data)
            ? is_array($data['unassembledMetrics'])
                ? array_values(array_map(static fn($item) => $item, $data['unassembledMetrics']))
                : []
            : [];
    }

    public static function fromArray(?array $data): ?self
    {
        return $data === null ? null : new self($data);
    }

    public function toArray(): array
    {
        return [
            'asOf' => $this->asOf,
            'platformScope' => $this->platformScope,
            'trafficSince' => $this->trafficSince,
            'windows' => array_values(array_map(static fn($item) => $item instanceof MetricsWindowBounds ? $item->toArray() : $item, $this->windows)),
            'entities' => array_values(array_map(static fn($item) => $item instanceof MetricsMetricTotals ? $item->toArray() : $item, $this->entities)),
            'traffic' => array_values(array_map(static fn($item) => $item instanceof MetricsMetricTotals ? $item->toArray() : $item, $this->traffic)),
            'storage' => array_values(array_map(static fn($item) => $item instanceof MetricsMetricTotals ? $item->toArray() : $item, $this->storage)),
            'series' => array_values(array_map(static fn($item) => $item instanceof MetricsSeries ? $item->toArray() : $item, $this->series)),
            'seriesWindow' => $this->seriesWindow instanceof MetricsSeriesWindow ? $this->seriesWindow->toArray() : $this->seriesWindow,
            'unassembledMetrics' => array_values(array_map(static fn($item) => $item, $this->unassembledMetrics)),
        ];
    }
}
