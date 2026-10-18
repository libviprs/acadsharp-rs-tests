/^    if "KNOWN_DIVERGENCE\] the listed divergence is gone" in body:$/ { print "    if \"KNOWN_DIVERGENCE] the listed divergence is gone\" in body or \"is in a different place\" in body or \"my multi line text for the attrrib\" in body:"; next }
/^    if rust:$/ && !done { print "    if \"ÄÖÜ\" in body and not rust:"; print "        return \"codepage\""; done=1 }
/^    \"other\": / { print "    \"codepage\": (\"the code page text of g11_codepage reads back as replacement characters\", issue(\"30\")),"; }
{ print }
