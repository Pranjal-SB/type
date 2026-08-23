# PSScriptAnalyzer is the shellcheck of PowerShell. It runs in CI against
# install.ps1 and its tests; see the install-script-windows job in ci.yml.
#
# Two rules are excluded, each because it is wrong for this code rather than
# because it was inconvenient. Anything else the analyser reports is a finding to
# fix, not an entry to add here.
@{
    Severity = @('Error', 'Warning')

    ExcludeRules = @(
        # PSAvoidUsingWriteHost wants Write-Output or Write-Information. Both are
        # wrong for an installer. Write-Output puts progress messages on the
        # pipeline, where `irm ... | iex` returns them to the caller as data.
        # Write-Information is invisible by default in 5.1 unless the user passes
        # -InformationAction, so the install would run silently. Write-Host is
        # the one that reliably reaches the human watching the terminal, which is
        # the entire audience for these lines.
        'PSAvoidUsingWriteHost',

        # PSUseShouldProcessForStateChangingFunctions wants -WhatIf/-Confirm on
        # anything named New-*. That is right for a published module and wrong
        # for two fixture helpers in a test file that exist to be called exactly
        # once each.
        'PSUseShouldProcessForStateChangingFunctions'
    )
}
