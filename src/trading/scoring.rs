//! Trading scoring system
//! Evaluates market quality, trading performance, portfolio risk,
//! and produces a composite score to guide trading decisions.

use serde::{Deserialize, Serialize};
use tracing::info;

use super::orderbook::OrderBookDepth;
use super::stats::TradingStats;

/// Score grade derived from a 0-100 numeric score
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoreGrade {
    /// 80-100
    Excellent,
    /// 60-79
    Good,
    /// 40-59
    Fair,
    /// 20-39
    Poor,
    /// 0-19
    Critical,
}

impl ScoreGrade {
    pub fn from_score(score: f64) -> Self {
        match score.round() as u32 {
            80..=100 => Self::Excellent,
            60..=79 => Self::Good,
            40..=59 => Self::Fair,
            20..=39 => Self::Poor,
            _ => Self::Critical,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Excellent => "Excellent",
            Self::Good => "Good",
            Self::Fair => "Fair",
            Self::Poor => "Poor",
            Self::Critical => "Critical",
        }
    }
}

// ---------------------------------------------------------------------------
// Market Score
// ---------------------------------------------------------------------------

/// Scores a market's suitability for trading based on order book metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketScore {
    /// Spread quality score (0-100, tighter spread = higher)
    pub spread_score: f64,
    /// Liquidity depth score (0-100, deeper books = higher)
    pub liquidity_score: f64,
    /// Order book balance score (0-100, balanced = higher)
    pub balance_score: f64,
    /// Price safety score (0-100, mid-range prices = higher)
    pub price_safety_score: f64,
    /// Overall market score (weighted average)
    pub total: f64,
    pub grade: ScoreGrade,
}

impl MarketScore {
    /// Evaluate a market from its order book depth.
    ///
    /// * `max_spread` – the spread at or above which the score is 0
    /// * `target_depth` – the depth at which liquidity score maxes out
    pub fn evaluate(depth: &OrderBookDepth, max_spread: f64, target_depth: f64) -> Self {
        // 1. Spread quality: 100 at 0 spread, 0 at max_spread
        let spread = depth.spread();
        let spread_score = if max_spread > 0.0 {
            ((1.0 - spread / max_spread) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        // 2. Liquidity: sum of bid+ask depth, capped at target_depth
        let total_depth = depth.bid_depth + depth.ask_depth;
        let liquidity_score = if target_depth > 0.0 {
            ((total_depth / target_depth) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        // 3. Balance: 100 when perfectly balanced, 0 when completely one-sided
        let balance_score = ((1.0 - depth.imbalance.abs()) * 100.0).clamp(0.0, 100.0);

        // 4. Price safety: highest at mid=0.50, decays toward 0.01 and 0.99
        let mid = depth.mid_price();
        let price_safety_score = if mid >= 0.01 && mid <= 0.99 {
            let dist_from_center = (mid - 0.5).abs(); // 0.0 best, 0.49 worst
            ((1.0 - dist_from_center / 0.49) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        // Weighted total
        let total = (spread_score * 0.35
            + liquidity_score * 0.25
            + balance_score * 0.20
            + price_safety_score * 0.20)
            .clamp(0.0, 100.0);
        let grade = ScoreGrade::from_score(total);

        Self {
            spread_score,
            liquidity_score,
            balance_score,
            price_safety_score,
            total,
            grade,
        }
    }

    /// Quick check: is this market worth trading?
    pub fn is_tradeable(&self, min_score: f64) -> bool {
        self.total >= min_score
    }
}

// ---------------------------------------------------------------------------
// Performance Score
// ---------------------------------------------------------------------------

/// Scores overall trading performance from aggregated statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceScore {
    /// Fill rate score (0-100): orders_filled / orders_placed
    pub fill_rate_score: f64,
    /// PnL score (0-100): based on total_pnl relative to volume
    pub pnl_score: f64,
    /// Error rate score (0-100, fewer errors = higher)
    pub error_rate_score: f64,
    /// Efficiency score (0-100): filled / (filled + cancelled + expired)
    pub efficiency_score: f64,
    /// Overall performance score
    pub total: f64,
    pub grade: ScoreGrade,
}

impl PerformanceScore {
    /// Evaluate performance from trading statistics.
    pub fn evaluate(stats: &TradingStats) -> Self {
        // Fill rate
        let fill_rate_score = if stats.orders_placed > 0 {
            ((stats.orders_filled as f64 / stats.orders_placed as f64) * 100.0).clamp(0.0, 100.0)
        } else {
            50.0 // neutral when no data
        };

        // PnL score: map return-on-volume to 0-100
        // +2% RoV -> 100, 0% -> 50, -2% -> 0
        let rov = if stats.total_volume > 0.0 {
            stats.total_pnl / stats.total_volume
        } else {
            0.0
        };
        let pnl_score = ((rov / 0.02 * 50.0) + 50.0).clamp(0.0, 100.0);

        // Error rate (lower is better)
        let total_actions = stats.orders_placed + stats.errors;
        let error_rate_score = if total_actions > 0 {
            ((1.0 - stats.errors as f64 / total_actions as f64) * 100.0).clamp(0.0, 100.0)
        } else {
            100.0
        };

        // Efficiency: filled / (filled + cancelled + expired)
        let terminal = stats.orders_filled + stats.orders_cancelled + stats.orders_expired;
        let efficiency_score = if terminal > 0 {
            ((stats.orders_filled as f64 / terminal as f64) * 100.0).clamp(0.0, 100.0)
        } else {
            50.0
        };

        let total = (fill_rate_score * 0.25
            + pnl_score * 0.35
            + error_rate_score * 0.20
            + efficiency_score * 0.20)
            .clamp(0.0, 100.0);
        let grade = ScoreGrade::from_score(total);

        Self {
            fill_rate_score,
            pnl_score,
            error_rate_score,
            efficiency_score,
            total,
            grade,
        }
    }
}

// ---------------------------------------------------------------------------
// Risk Score
// ---------------------------------------------------------------------------

/// Scores current portfolio risk level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScore {
    /// Inventory skew score (0-100, balanced = higher)
    pub skew_score: f64,
    /// Exposure score (0-100, lower exposure relative to limit = higher)
    pub exposure_score: f64,
    /// Concentration score (0-100, diversified positions = higher)
    pub concentration_score: f64,
    /// Overall risk score (higher = lower risk = safer)
    pub total: f64,
    pub grade: ScoreGrade,
}

impl RiskScore {
    /// Evaluate portfolio risk.
    ///
    /// * `inventory_skew` – current skew from -1 to 1
    /// * `total_exposure` – current total position value
    /// * `max_exposure` – configured maximum position value
    /// * `num_positions` – number of open positions
    /// * `max_positions` – soft cap for diversification scoring
    pub fn evaluate(
        inventory_skew: f64,
        total_exposure: f64,
        max_exposure: f64,
        num_positions: usize,
        max_positions: usize,
    ) -> Self {
        // Skew: 100 at 0, 0 at ±1
        let skew_score = ((1.0 - inventory_skew.abs()) * 100.0).clamp(0.0, 100.0);

        // Exposure: 100 at 0% utilisation, 0 at 100%+
        let exposure_ratio = if max_exposure > 0.0 {
            total_exposure / max_exposure
        } else {
            1.0
        };
        let exposure_score = ((1.0 - exposure_ratio) * 100.0).clamp(0.0, 100.0);

        // Concentration: penalise having too few (<2) or too many (>max) positions
        let concentration_score = if max_positions == 0 {
            50.0
        } else if num_positions == 0 {
            100.0 // no positions = no concentration risk
        } else {
            let ideal = max_positions as f64 / 2.0;
            let deviation = (num_positions as f64 - ideal).abs() / ideal;
            ((1.0 - deviation) * 100.0).clamp(0.0, 100.0)
        };

        let total = (skew_score * 0.40 + exposure_score * 0.35 + concentration_score * 0.25)
            .clamp(0.0, 100.0);
        let grade = ScoreGrade::from_score(total);

        Self {
            skew_score,
            exposure_score,
            concentration_score,
            total,
            grade,
        }
    }

    /// Is the portfolio in a safe state?
    pub fn is_safe(&self, min_score: f64) -> bool {
        self.total >= min_score
    }
}

// ---------------------------------------------------------------------------
// Composite Score
// ---------------------------------------------------------------------------

/// Combines market, performance, and risk scores into one overall assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeScore {
    pub market: MarketScore,
    pub performance: PerformanceScore,
    pub risk: RiskScore,
    /// Overall composite score (0-100)
    pub total: f64,
    pub grade: ScoreGrade,
    pub timestamp: String,
}

impl CompositeScore {
    /// Build a composite score from the three sub-scores.
    pub fn new(market: MarketScore, performance: PerformanceScore, risk: RiskScore) -> Self {
        let total = (market.total * 0.30 + performance.total * 0.35 + risk.total * 0.35)
            .clamp(0.0, 100.0);
        let grade = ScoreGrade::from_score(total);
        let timestamp = chrono::Utc::now().to_rfc3339();

        Self {
            market,
            performance,
            risk,
            total,
            grade,
            timestamp,
        }
    }

    /// Should the bot continue aggressive trading?
    pub fn should_trade_aggressively(&self) -> bool {
        self.total >= 70.0
    }

    /// Should the bot reduce activity (e.g., widen spreads, skip cycles)?
    pub fn should_reduce_activity(&self) -> bool {
        self.total < 40.0
    }

    /// Human-readable one-line summary.
    pub fn summary(&self) -> String {
        format!(
            "Score: {:.0}/100 [{}] | Market {:.0} | Perf {:.0} | Risk {:.0}",
            self.total,
            self.grade.label(),
            self.market.total,
            self.performance.total,
            self.risk.total,
        )
    }

    /// Log the composite score.
    pub fn log(&self) {
        info!("{}", self.summary());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trading::orderbook::{OrderBookDepth, OrderBookLevel};

    fn sample_depth() -> OrderBookDepth {
        OrderBookDepth {
            best_bid: OrderBookLevel {
                price: 0.49,
                size: 500.0,
            },
            best_ask: OrderBookLevel {
                price: 0.51,
                size: 500.0,
            },
            second_bid: OrderBookLevel {
                price: 0.48,
                size: 300.0,
            },
            second_ask: OrderBookLevel {
                price: 0.52,
                size: 300.0,
            },
            bid_depth: 800.0,
            ask_depth: 800.0,
            imbalance: 0.0,
        }
    }

    // -- MarketScore --

    #[test]
    fn test_market_score_excellent() {
        let depth = sample_depth();
        let score = MarketScore::evaluate(&depth, 0.05, 1000.0);
        // tight spread, good depth, balanced, mid-price ~0.50
        assert!(score.total >= 80.0, "expected excellent, got {:.1}", score.total);
        assert_eq!(score.grade, ScoreGrade::Excellent);
        assert!(score.is_tradeable(60.0));
    }

    #[test]
    fn test_market_score_wide_spread() {
        let mut depth = sample_depth();
        depth.best_bid.price = 0.40;
        depth.best_ask.price = 0.60;
        let score = MarketScore::evaluate(&depth, 0.05, 1000.0);
        // spread=0.20, way beyond max_spread=0.05 -> spread_score=0
        assert!(score.spread_score <= 1.0);
        assert!(score.total < 80.0);
    }

    #[test]
    fn test_market_score_imbalanced() {
        let mut depth = sample_depth();
        depth.imbalance = 0.8;
        let score = MarketScore::evaluate(&depth, 0.05, 1000.0);
        assert!(score.balance_score < 30.0);
    }

    #[test]
    fn test_market_score_extreme_price() {
        let mut depth = sample_depth();
        depth.best_bid.price = 0.96;
        depth.best_ask.price = 0.98;
        let score = MarketScore::evaluate(&depth, 0.05, 1000.0);
        assert!(score.price_safety_score < 20.0);
    }

    // -- PerformanceScore --

    #[test]
    fn test_performance_score_no_data() {
        let stats = TradingStats::new();
        let score = PerformanceScore::evaluate(&stats);
        // neutral values
        assert!((score.fill_rate_score - 50.0).abs() < 1.0);
        assert!((score.pnl_score - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_performance_score_good() {
        let mut stats = TradingStats::new();
        stats.orders_placed = 100;
        stats.orders_filled = 80;
        stats.orders_cancelled = 10;
        stats.orders_expired = 10;
        stats.total_volume = 1000.0;
        stats.total_pnl = 15.0; // +1.5% RoV
        stats.errors = 2;

        let score = PerformanceScore::evaluate(&stats);
        assert!(score.fill_rate_score >= 75.0);
        assert!(score.pnl_score > 50.0);
        assert!(score.error_rate_score > 90.0);
        assert!(score.efficiency_score >= 75.0);
        assert!(score.total > 60.0);
    }

    #[test]
    fn test_performance_score_losing() {
        let mut stats = TradingStats::new();
        stats.orders_placed = 50;
        stats.orders_filled = 10;
        stats.orders_cancelled = 30;
        stats.orders_expired = 10;
        stats.total_volume = 500.0;
        stats.total_pnl = -20.0; // -4% RoV
        stats.errors = 15;

        let score = PerformanceScore::evaluate(&stats);
        assert!(score.pnl_score < 50.0);
        assert!(score.fill_rate_score < 30.0);
    }

    // -- RiskScore --

    #[test]
    fn test_risk_score_safe() {
        let score = RiskScore::evaluate(0.0, 5.0, 30.0, 3, 10);
        assert!(score.skew_score >= 99.0);
        assert!(score.exposure_score > 80.0);
        assert!(score.total > 70.0);
        assert!(score.is_safe(60.0));
    }

    #[test]
    fn test_risk_score_high_skew() {
        let score = RiskScore::evaluate(0.9, 5.0, 30.0, 3, 10);
        assert!(score.skew_score < 15.0);
        assert!(score.total < 70.0);
    }

    #[test]
    fn test_risk_score_high_exposure() {
        let score = RiskScore::evaluate(0.0, 28.0, 30.0, 3, 10);
        assert!(score.exposure_score < 10.0);
    }

    #[test]
    fn test_risk_score_no_positions() {
        let score = RiskScore::evaluate(0.0, 0.0, 30.0, 0, 10);
        assert!(score.total >= 90.0);
    }

    // -- CompositeScore --

    #[test]
    fn test_composite_score() {
        let depth = sample_depth();
        let market = MarketScore::evaluate(&depth, 0.05, 1000.0);
        let stats = TradingStats::new();
        let perf = PerformanceScore::evaluate(&stats);
        let risk = RiskScore::evaluate(0.0, 5.0, 30.0, 2, 10);

        let composite = CompositeScore::new(market, perf, risk);
        assert!(composite.total > 0.0 && composite.total <= 100.0);
        assert!(!composite.summary().is_empty());
    }

    #[test]
    fn test_composite_aggressive_threshold() {
        let market = MarketScore {
            spread_score: 90.0,
            liquidity_score: 90.0,
            balance_score: 90.0,
            price_safety_score: 90.0,
            total: 90.0,
            grade: ScoreGrade::Excellent,
        };
        let perf = PerformanceScore {
            fill_rate_score: 80.0,
            pnl_score: 80.0,
            error_rate_score: 95.0,
            efficiency_score: 80.0,
            total: 82.0,
            grade: ScoreGrade::Excellent,
        };
        let risk = RiskScore {
            skew_score: 90.0,
            exposure_score: 80.0,
            concentration_score: 70.0,
            total: 82.0,
            grade: ScoreGrade::Excellent,
        };
        let composite = CompositeScore::new(market, perf, risk);
        assert!(composite.should_trade_aggressively());
        assert!(!composite.should_reduce_activity());
    }

    #[test]
    fn test_composite_reduce_activity() {
        let market = MarketScore {
            spread_score: 10.0,
            liquidity_score: 10.0,
            balance_score: 10.0,
            price_safety_score: 10.0,
            total: 10.0,
            grade: ScoreGrade::Critical,
        };
        let perf = PerformanceScore {
            fill_rate_score: 20.0,
            pnl_score: 20.0,
            error_rate_score: 50.0,
            efficiency_score: 20.0,
            total: 25.0,
            grade: ScoreGrade::Poor,
        };
        let risk = RiskScore {
            skew_score: 10.0,
            exposure_score: 10.0,
            concentration_score: 30.0,
            total: 14.0,
            grade: ScoreGrade::Critical,
        };
        let composite = CompositeScore::new(market, perf, risk);
        assert!(!composite.should_trade_aggressively());
        assert!(composite.should_reduce_activity());
    }

    // -- ScoreGrade --

    #[test]
    fn test_score_grade_boundaries() {
        assert_eq!(ScoreGrade::from_score(100.0), ScoreGrade::Excellent);
        assert_eq!(ScoreGrade::from_score(80.0), ScoreGrade::Excellent);
        assert_eq!(ScoreGrade::from_score(79.0), ScoreGrade::Good);
        assert_eq!(ScoreGrade::from_score(60.0), ScoreGrade::Good);
        assert_eq!(ScoreGrade::from_score(59.0), ScoreGrade::Fair);
        assert_eq!(ScoreGrade::from_score(40.0), ScoreGrade::Fair);
        assert_eq!(ScoreGrade::from_score(39.0), ScoreGrade::Poor);
        assert_eq!(ScoreGrade::from_score(20.0), ScoreGrade::Poor);
        assert_eq!(ScoreGrade::from_score(19.0), ScoreGrade::Critical);
        assert_eq!(ScoreGrade::from_score(0.0), ScoreGrade::Critical);
    }
}
