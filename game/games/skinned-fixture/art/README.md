# fox.glb

The Fox from the Khronos glTF Sample Assets
(<https://github.com/KhronosGroup/glTF-Sample-Assets/tree/main/Models/Fox>),
under the licences in `Fox-LICENSE.md`:

- the model, by PixelMannen (2014), under CC0 1.0;
- the rigging and animation, © 2014 tomkranis, under CC BY 4.0;
- the conversion to glTF, © 2017 @AsoboStudio and @scurest, under CC BY 4.0
  (<https://creativecommons.org/licenses/by/4.0/legalcode>).

This copy is modified. It adds `asset.extras.metersPerUnit` (0.025);
`extras.markers` naming the steps of the Walk and Run animations; and, in each
of those two, a translation channel on the root bone `b_Root_00`, with its
sampler, two accessors and two buffer views, their data appended to the
buffer. The rest is the sample's.
