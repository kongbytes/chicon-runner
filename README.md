```sh
# Setup containrd as root with SH script
# CGROUPv2 not required now

# Download CNI plugins
https://github.com/containernetworking/plugins/releases/tag/v1.0.1
# Put them in /opt/cni/bin
```

Function can return 3 things
- **metrics** : they return a KPI about the repository (usage: display, dashboards, queries, ...)
    - high_vulns=3
    - last_update=(date)
    - has_nodejs_version=true
- **raw records** : bulk repository data (usage: search, ...)
    - package lock list
    - files list
    - routes list
- **findings** : more focused on a file/line they return a specific concern (usage: code quality, security, ...)
    - security hotspot
    - lint failure
    - leaked credential
