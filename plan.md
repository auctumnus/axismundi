things left to do:
- bugfix the cognacy model (wow that is scary)
- docs (openapi+something nicer?)
- ensure all endpoints feel consistent
- no reachable todo!() or unimplemented!()
- reduce panic surface or figure out if we can horizontally scale
- frontend
  - @mention support is really really janky rn
- real mail with resend
- really need to standardize how to handle the way form fields are submitted from html;
  atm i think its basically confusing Option<String> for Option<Option<String>>

  