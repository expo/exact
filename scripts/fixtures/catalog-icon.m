// Test-only CoreUI icon loading, beside public UIKit color and image loading.
#import <UIKit/UIKit.h>
#import <dlfcn.h>
#import <math.h>
@interface NSObject (CatalogProbe)
- (instancetype)initWithURL:(NSURL *)url error:(NSError **)error;
- (id)iconImageWithName:(NSString *)name scaleFactor:(double)scale deviceIdiom:(long long)idiom deviceSubtype:(unsigned long long)subtype displayGamut:(long long)gamut layoutDirection:(long long)direction sizeClassHorizontal:(long long)horizontal sizeClassVertical:(long long)vertical desiredSize:(CGSize)size;
@end
@interface ExactNamedIcon : NSObject
- (CGImageRef)image;
@end
int main(int argc,char **argv) { @autoreleasepool {
 if(argc!=2)return 1;
 if(!dlopen("/System/Library/PrivateFrameworks/CoreUI.framework/CoreUI",RTLD_NOW))return 2;
 NSError *error=nil;id catalog=[[NSClassFromString(@"CUICatalog") alloc] initWithURL:[NSURL fileURLWithPath:[NSString stringWithUTF8String:argv[1]]] error:&error];
 if(!catalog){NSLog(@"Catalog failed %@",error);return 3;}
 for(NSNumber *idiom in @[@1,@2])for(NSString *name in @[@"AppIcon",@"Alternate"]){
  ExactNamedIcon *asset=[catalog iconImageWithName:name scaleFactor:3 deviceIdiom:idiom.longLongValue deviceSubtype:0 displayGamut:1 layoutDirection:0 sizeClassHorizontal:0 sizeClassVertical:0 desiredSize:CGSizeMake(60,60)];
  CGImageRef image=[asset image];if(!image){NSLog(@"Missing %@ %@",name,idiom);return 4;}
  unsigned char bytes[4]={0};CGColorSpaceRef space=CGColorSpaceCreateWithName(kCGColorSpaceSRGB);CGContextRef ctx=CGBitmapContextCreate(bytes,1,1,8,4,space,(CGBitmapInfo)kCGImageAlphaPremultipliedLast);CGContextDrawImage(ctx,CGRectMake(0,0,1,1),image);CGContextRelease(ctx);CGColorSpaceRelease(space);
  BOOL red=[name isEqualToString:@"AppIcon"];if(bytes[red?0:2]!=255 || bytes[1]!=0 || bytes[3]!=255 || bytes[red?2:0]!=0){NSLog(@"Wrong pixels %u %u %u %u",bytes[0],bytes[1],bytes[2],bytes[3]);return 5;}
  printf("%s idiom %lld: %zux%zu pixels OK\n",name.UTF8String,idiom.longLongValue,CGImageGetWidth(image),CGImageGetHeight(image));
 }
 NSBundle *bundle=[NSBundle bundleWithPath:[[NSString stringWithUTF8String:argv[1]] stringByDeletingLastPathComponent]];
 UITraitCollection *dark=[UITraitCollection traitCollectionWithUserInterfaceStyle:UIUserInterfaceStyleDark];
 UIColor *color=[[UIColor colorNamed:@"Demo" inBundle:bundle compatibleWithTraitCollection:dark] resolvedColorWithTraitCollection:dark];
 CGFloat r=0,g=0,b=0,a=0;if(![color getRed:&r green:&g blue:&b alpha:&a] || fabs(r-.6)>.0001 || fabs(a-1)>.0001){NSLog(@"Wrong mixed color: %@ %f %f %f %f",color,r,g,b,a);return 6;}
 UIImage *picture=[UIImage imageNamed:@"Picture" inBundle:bundle compatibleWithTraitCollection:[UITraitCollection traitCollectionWithDisplayScale:3]];
 if(!picture || picture.scale!=3 || CGImageGetWidth(picture.CGImage)!=6)return 7;
 printf("Mixed catalog colors and image scales passed\n");
 return 0;
}}
