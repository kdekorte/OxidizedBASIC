OxidizedBASIC macOS icon package

Files:
- OxidizedBASIC_1024.png: master PNG (1024x1024)
- OxidizedBASIC.icns: legacy macOS ICNS icon file
- OxidizedBASIC.iconset/: PNG sizes for iconutil (including Retina variants)
- Assets.xcassets/OxidizedBASIC.appiconset/: Xcode asset catalog entry

Xcode: add Assets.xcassets to the project, then set the target App Icon to OxidizedBASIC.

Manual .icns rebuild on macOS (if desired):
  iconutil -c icns OxidizedBASIC.iconset -o OxidizedBASIC.icns

Note: this package uses the generated concept artwork as-is, resized and packaged.
