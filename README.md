This branch was created to test i2p support using a custom scheme.
This works, but many eepsites are broken because they assume to run in a browser that uses http instead of i2p, and we can't intercept those http requests without a webkit extension.

## Requirements

```
sudo apt install libglib2.0-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev
```
