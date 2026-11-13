package main

import "tailscale.com/feature/buildfeatures"

// omittedFeaturesPresent names the Tailscale features the release build leaves out that are
// still compiled in. The helper refuses to run with any of them; tests call run directly.
func omittedFeaturesPresent() []string {
	features := []struct {
		name    string
		present bool
	}{
		{"logtail", buildfeatures.HasLogTail},
		{"netlog", buildfeatures.HasNetLog},
		{"clientupdate", buildfeatures.HasClientUpdate},
		{"c2n", buildfeatures.HasC2N},
		{"ssh", buildfeatures.HasSSH},
		{"taildrop", buildfeatures.HasTaildrop},
		{"drive", buildfeatures.HasDrive},
		{"webclient", buildfeatures.HasWebClient},
		{"peerapiserver", buildfeatures.HasPeerAPIServer},
		{"relayserver", buildfeatures.HasRelayServer},
		{"remoteconfig", buildfeatures.HasRemoteConfig},
		{"outboundproxy", buildfeatures.HasOutboundProxy},
		{"advertiseroutes", buildfeatures.HasAdvertiseRoutes},
		{"advertiseexitnode", buildfeatures.HasAdvertiseExitNode},
		{"captiveportal", buildfeatures.HasCaptivePortal},
		{"portmapper", buildfeatures.HasPortMapper},
		{"capture", buildfeatures.HasCapture},
		{"debug", buildfeatures.HasDebug},
		{"syspolicy", buildfeatures.HasSystemPolicy},
		{"clientmetrics", buildfeatures.HasClientMetrics},
		{"usermetrics", buildfeatures.HasUserMetrics},
	}
	var present []string
	for _, f := range features {
		if f.present {
			present = append(present, f.name)
		}
	}
	return present
}
