package main

import (
	"math"
	"sort"
)

// HMMStrategy fits a Gaussian-emission Hidden Markov Model with HiddenStates
// hidden states on a training window of returns (first ~TrainFrac of bars),
// then runs the forward algorithm online to label each subsequent bar with
// its most-likely hidden state. The hidden states are sorted by their fitted
// emission mean — lowest-mean state → Short, middle → Hold, highest → Buy.
//
// Why fit only once on a training window (rather than walk-forward Baum-Welch
// per bar): full Baum-Welch is O(N²K²) per fit; redoing it every bar would
// dominate runtime. Fitting on a held-out prefix and freezing the model is
// the standard pragmatic choice — and the strategy emits Hold during the
// training window so there is no lookahead.
type HMMStrategy struct {
	HiddenStates int     // 3
	TrainFrac    float64 // 0.30 — fraction of bars used to fit the model
	MinTrainBars int     // 60 — minimum training-window length
	MaxIter      int     // 50 — Baum-Welch iteration cap
	Tol          float64 // 1e-4 — convergence threshold on ΔlogL
	ConfThresh   float64 // 0.60 — required posterior probability before emitting a directional signal
}

func NewHMMStrategy() *HMMStrategy {
	return &HMMStrategy{HiddenStates: 3, TrainFrac: 0.30, MinTrainBars: 60, MaxIter: 50, Tol: 1e-4, ConfThresh: 0.60}
}

func (h *HMMStrategy) Name() string { return "HMM" }

func (h *HMMStrategy) GenerateSignals(bars []Bar) []Signal {
	signals := make([]Signal, len(bars))
	k := h.HiddenStates
	if k < 2 || len(bars) < 30 {
		return signals
	}

	trainN := int(float64(len(bars)) * h.TrainFrac)
	if trainN < h.MinTrainBars {
		trainN = h.MinTrainBars
	}
	// Need a sane held-out section AND enough training data.
	if len(bars) < trainN+10 {
		return signals
	}

	rets := returnsFromBars(bars)
	// Training observations: skip index 0 (always zero) and use rets[1:trainN].
	if trainN < 3 {
		return signals
	}
	trainObs := rets[1:trainN]

	pi, A, mu, sigma, ok := fitHMMGaussian(trainObs, k, h.MaxIter, h.Tol)
	if !ok {
		return signals
	}

	// Sort hidden-state indices by emission mean ascending so we can map
	// state→signal deterministically: lowest mean → Short, highest → Buy.
	order := make([]int, k)
	for i := range order {
		order[i] = i
	}
	sort.Slice(order, func(i, j int) bool { return mu[order[i]] < mu[order[j]] })
	stateRole := make([]int, k) // -1 = short bias, 0 = hold, +1 = long bias
	for rank, idx := range order {
		switch {
		case rank == 0:
			stateRole[idx] = -1
		case rank == k-1:
			stateRole[idx] = 1
		default:
			stateRole[idx] = 0
		}
	}

	// Online forward filter for the post-training segment. We rebuild the
	// full log-alpha from rets[1:i+1] each bar — O(N²K²) total. For the
	// largest practical window (~2500 bars × 9 = ~56M ops) this is fine.
	confThresh := h.ConfThresh
	for i := trainN; i < len(bars); i++ {
		obs := rets[1 : i+1]
		logAlpha := hmmForwardLog(obs, pi, A, mu, sigma)
		if logAlpha == nil {
			continue
		}
		last := logAlpha[len(logAlpha)-1]
		// Convert to normalised posteriors via logSumExp so we can compare
		// against the confidence threshold cleanly.
		norm := logSumExp(last)
		argmax, bestProb := 0, 0.0
		for s := 0; s < k; s++ {
			p := math.Exp(last[s] - norm)
			if p > bestProb {
				bestProb = p
				argmax = s
			}
		}
		if bestProb < confThresh {
			continue // not confident enough — hold
		}
		switch stateRole[argmax] {
		case 1:
			signals[i] = SignalBuy
		case -1:
			signals[i] = SignalShort
		}
	}
	return signals
}

// logSumExp returns log(Σ exp(x_i)) using the max-shift trick so the sum
// is numerically stable even when the values are large or very negative.
// An empty input returns -Inf (the additive identity in log space).
func logSumExp(xs []float64) float64 {
	if len(xs) == 0 {
		return math.Inf(-1)
	}
	m := xs[0]
	for _, v := range xs[1:] {
		if v > m {
			m = v
		}
	}
	if math.IsInf(m, -1) {
		return m
	}
	var sum float64
	for _, v := range xs {
		sum += math.Exp(v - m)
	}
	return m + math.Log(sum)
}

// gaussianLogPDF is the log of the univariate Gaussian density at x with
// the given mean and stddev. sigma is clamped to a tiny floor by the
// caller — Baum-Welch enforces this via the variance floor — but we add a
// belt-and-braces check here too.
func gaussianLogPDF(x, mu, sigma float64) float64 {
	if sigma < 1e-12 {
		sigma = 1e-12
	}
	d := x - mu
	return -0.5*math.Log(2*math.Pi) - math.Log(sigma) - 0.5*(d*d)/(sigma*sigma)
}

// fitHMMGaussian runs Baum-Welch on a Gaussian-emission HMM with k hidden
// states. Entirely in log space. Returns the fitted (pi, A, mu, sigma) and
// ok=false if the fit goes off the rails (NaN/Inf, no improvement, etc.).
//
// Init is deterministic: pi uniform, A diagonally biased (0.8 on the
// diagonal, the remainder split evenly off-diagonal), mu = evenly-spaced
// quantiles of the training data, sigma = global stddev for every state.
// Tests need reproducibility, so no RNG.
func fitHMMGaussian(obs []float64, k, maxIter int, tol float64) (pi []float64, A [][]float64, mu, sigma []float64, ok bool) {
	T := len(obs)
	if T < k+2 || k < 1 {
		return nil, nil, nil, nil, false
	}

	// ── Initialization ──
	pi = make([]float64, k)
	for i := range pi {
		pi[i] = 1.0 / float64(k)
	}

	A = make([][]float64, k)
	offDiag := 0.2 / float64(k-1)
	if k == 1 {
		offDiag = 0
	}
	for i := range A {
		A[i] = make([]float64, k)
		for j := range A[i] {
			if i == j {
				A[i][j] = 0.8
			} else {
				A[i][j] = offDiag
			}
		}
	}

	sorted := append([]float64(nil), obs...)
	sort.Float64s(sorted)
	mu = make([]float64, k)
	for i := 0; i < k; i++ {
		// Evenly-spaced quantiles: (i+1)/(k+1) ∈ {0.25, 0.5, 0.75} for k=3.
		qIdx := int(float64(i+1) / float64(k+1) * float64(T-1))
		mu[i] = sorted[qIdx]
	}

	var meanAll, sumSq float64
	for _, v := range obs {
		meanAll += v
	}
	meanAll /= float64(T)
	for _, v := range obs {
		d := v - meanAll
		sumSq += d * d
	}
	globalSigma := math.Sqrt(sumSq / float64(T))
	if globalSigma < 1e-6 {
		globalSigma = 1e-6
	}
	sigma = make([]float64, k)
	for i := range sigma {
		sigma[i] = globalSigma
	}

	logPi := make([]float64, k)
	logA := make([][]float64, k)
	for i := range logA {
		logA[i] = make([]float64, k)
	}
	syncLogParams := func() {
		for i := 0; i < k; i++ {
			logPi[i] = safeLog(pi[i])
			for j := 0; j < k; j++ {
				logA[i][j] = safeLog(A[i][j])
			}
		}
	}
	syncLogParams()

	logAlpha := make([][]float64, T)
	logBeta := make([][]float64, T)
	for t := 0; t < T; t++ {
		logAlpha[t] = make([]float64, k)
		logBeta[t] = make([]float64, k)
	}

	prevLogL := math.Inf(-1)
	noImprove := 0

	for iter := 0; iter < maxIter; iter++ {
		// ── Forward ──
		for s := 0; s < k; s++ {
			logAlpha[0][s] = logPi[s] + gaussianLogPDF(obs[0], mu[s], sigma[s])
		}
		buf := make([]float64, k)
		for t := 1; t < T; t++ {
			emit := make([]float64, k)
			for s := 0; s < k; s++ {
				emit[s] = gaussianLogPDF(obs[t], mu[s], sigma[s])
			}
			for s := 0; s < k; s++ {
				for j := 0; j < k; j++ {
					buf[j] = logAlpha[t-1][j] + logA[j][s]
				}
				logAlpha[t][s] = logSumExp(buf) + emit[s]
			}
		}
		logL := logSumExp(logAlpha[T-1])
		if math.IsNaN(logL) || math.IsInf(logL, 0) {
			return nil, nil, nil, nil, false
		}

		if iter > 0 {
			if logL-prevLogL < tol {
				noImprove++
				if noImprove >= 3 {
					break
				}
			} else {
				noImprove = 0
			}
		}
		prevLogL = logL

		// ── Backward ──
		for s := 0; s < k; s++ {
			logBeta[T-1][s] = 0
		}
		for t := T - 2; t >= 0; t-- {
			emit := make([]float64, k)
			for s := 0; s < k; s++ {
				emit[s] = gaussianLogPDF(obs[t+1], mu[s], sigma[s])
			}
			for s := 0; s < k; s++ {
				for j := 0; j < k; j++ {
					buf[j] = logA[s][j] + emit[j] + logBeta[t+1][j]
				}
				logBeta[t][s] = logSumExp(buf)
			}
		}

		// ── Posteriors (gamma, xi) and parameter updates ──
		gammaSum := make([]float64, k) // sum_t exp(logγ[t][s])  (t = 0..T-1)
		gammaSumExcLast := make([]float64, k) // sum over t = 0..T-2 for A normaliser
		muNum := make([]float64, k)
		xiSum := make([][]float64, k)
		for i := range xiSum {
			xiSum[i] = make([]float64, k)
		}

		gammaT0 := make([]float64, k)
		for s := 0; s < k; s++ {
			gammaT0[s] = math.Exp(logAlpha[0][s] + logBeta[0][s] - logL)
		}

		emit := make([]float64, k)
		for t := 0; t < T; t++ {
			for s := 0; s < k; s++ {
				g := math.Exp(logAlpha[t][s] + logBeta[t][s] - logL)
				gammaSum[s] += g
				muNum[s] += g * obs[t]
				if t < T-1 {
					gammaSumExcLast[s] += g
				}
			}
			if t < T-1 {
				for s := 0; s < k; s++ {
					emit[s] = gaussianLogPDF(obs[t+1], mu[s], sigma[s])
				}
				for i := 0; i < k; i++ {
					for j := 0; j < k; j++ {
						xiSum[i][j] += math.Exp(logAlpha[t][i] + logA[i][j] + emit[j] + logBeta[t+1][j] - logL)
					}
				}
			}
		}

		// pi
		for s := 0; s < k; s++ {
			pi[s] = gammaT0[s]
		}
		normalize(pi)

		// A
		for i := 0; i < k; i++ {
			denom := gammaSumExcLast[i]
			if denom <= 0 {
				for j := 0; j < k; j++ {
					A[i][j] = 1.0 / float64(k)
				}
				continue
			}
			for j := 0; j < k; j++ {
				A[i][j] = xiSum[i][j] / denom
			}
			normalize(A[i])
		}

		// mu, sigma (variance floor at 1e-6 ⇒ sigma floor at 1e-3)
		for s := 0; s < k; s++ {
			if gammaSum[s] <= 0 {
				continue
			}
			newMu := muNum[s] / gammaSum[s]
			var varNum float64
			for t := 0; t < T; t++ {
				g := math.Exp(logAlpha[t][s] + logBeta[t][s] - logL)
				d := obs[t] - newMu
				varNum += g * d * d
			}
			variance := varNum / gammaSum[s]
			if variance < 1e-6 {
				variance = 1e-6
			}
			mu[s] = newMu
			sigma[s] = math.Sqrt(variance)
		}

		syncLogParams()
	}

	// Sanity: every fitted parameter must be finite.
	for i := 0; i < k; i++ {
		if !finite(pi[i]) || !finite(mu[i]) || !finite(sigma[i]) {
			return nil, nil, nil, nil, false
		}
		for j := 0; j < k; j++ {
			if !finite(A[i][j]) {
				return nil, nil, nil, nil, false
			}
		}
	}
	return pi, A, mu, sigma, true
}

// hmmForwardLog returns the log-alpha matrix for the supplied observation
// sequence under the given Gaussian HMM parameters. Returns nil if any
// parameter is malformed (caller treats nil as "no signal this bar").
func hmmForwardLog(obs []float64, pi []float64, A [][]float64, mu, sigma []float64) [][]float64 {
	T := len(obs)
	k := len(pi)
	if T == 0 || k == 0 {
		return nil
	}
	logAlpha := make([][]float64, T)
	for t := 0; t < T; t++ {
		logAlpha[t] = make([]float64, k)
	}
	for s := 0; s < k; s++ {
		logAlpha[0][s] = safeLog(pi[s]) + gaussianLogPDF(obs[0], mu[s], sigma[s])
	}
	buf := make([]float64, k)
	for t := 1; t < T; t++ {
		for s := 0; s < k; s++ {
			for j := 0; j < k; j++ {
				buf[j] = logAlpha[t-1][j] + safeLog(A[j][s])
			}
			logAlpha[t][s] = logSumExp(buf) + gaussianLogPDF(obs[t], mu[s], sigma[s])
		}
	}
	return logAlpha
}

func safeLog(x float64) float64 {
	if x <= 0 {
		return math.Log(1e-12)
	}
	return math.Log(x)
}

func normalize(v []float64) {
	var s float64
	for _, x := range v {
		s += x
	}
	if s <= 0 {
		uniform := 1.0 / float64(len(v))
		for i := range v {
			v[i] = uniform
		}
		return
	}
	for i := range v {
		v[i] /= s
	}
}

func finite(x float64) bool { return !math.IsNaN(x) && !math.IsInf(x, 0) }
