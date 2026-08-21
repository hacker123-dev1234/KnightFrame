# KnightFrame brand assets

KnightFrame has two deliberately separate brand layers:

- `knightframe-ui-hero-master.png` is the locked, high-detail UI hero. Use its
  transparent derivatives for internal UI artwork displayed at 64 px or larger.
  Its canvas and composition are unchanged from the supplied artwork.
- `knightframe-primary-master.png` is the locked compact heraldic mark. It alone
  drives ICO and compact marks. `knightframe-primary-backup-approved.png` is its
  byte-identical approved source. `knightframe-primary-hard-alt.png` preserves
  the former hard-edged compact artwork as reference-only; it must never enter
  the shipping generator. This mapping is final and frozen.

No master may be edited. Rejected v2/v3/v4/v5 drafts are not part of the
deliverable and are never inputs to either generator.

## Shipping assets

| File | Use |
| --- | --- |
| `knightframe-primary-black-transparent.png` | Black primary mark on a transparent canvas |
| `knightframe-primary-white-transparent.png` | White primary mark on a transparent canvas |
| `knightframe-app-light-1024.png` | Black primary mark on an opaque white app-icon canvas |
| `knightframe-app-dark-1024.png` | White primary mark on an opaque black app-icon canvas |
| `knightframe-app-light.ico` | Windows light icon with 16, 20, 24, 28, 32, 48, 64, 96, 128 and 256 px frames |
| `knightframe-app-dark.ico` | Windows dark icon with the same frame set |
| `brand-assets.manifest.json` | Master identity, frame policy and SHA-256 inventory for generated assets |
| `knightframe-ui-hero-black-transparent.png` | High-detail black UI hero on transparency, for light surfaces at 64 px+ |
| `knightframe-ui-hero-white-transparent.png` | High-detail white UI hero on transparency, for dark surfaces at 64 px+ |

`generate_brand_assets.py` deterministically rebuilds these files from the
production master. Frames at 16, 20, 24, 28, 32 and 48 px receive optical-size
fitting and slight negative-space/contrast protection so existing gaps survive
rasterization. Frames at 64 px and above are direct master derivatives with no
optical erosion or contrast adjustment. Neither path changes the pose.

`generate_ui_hero_assets.py` rebuilds only the two high-detail hero derivatives
and appends their immutable source identity to the manifest. It never invokes or
overwrites the compact-mark pipeline.

## Validation assets

`knightframe-small-size-validation.png` shows native 16, 20, 24, 28, 32, 48
and 64 px frames enlarged with nearest-neighbour sampling. The matching native
PNG frames live in `validation/`. These are review evidence, not application
resources.

At each reviewed size the following must remain recognizable:

- the raised sword and knight's helmet;
- both raised horse forelegs;
- the rider leg separated from the horse's rear legs;
- the horse head, ears and mane.

The Sword Gate empty-state artwork is a separate, protected logo. It is not
stored, generated or modified here.
