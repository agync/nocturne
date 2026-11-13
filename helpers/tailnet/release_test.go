//go:build ts_omit_logtail

package main

import "testing"

func TestReleaseTagsOmitEveryCheckedFeature(t *testing.T) {
	if present := omittedFeaturesPresent(); len(present) != 0 {
		t.Fatalf("the release tags leave in: %q", present)
	}
}
