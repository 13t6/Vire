# Generates the built-in seamless-loop wallpaper (1920x1080, 30fps, 12s) with ffmpeg.
param([string]$Out = "$PSScriptRoot\..\samples")
New-Item -ItemType Directory -Force $Out | Out-Null
$enc = @('-c:v','libx264','-preset','slow','-crf','24','-pix_fmt','yuv420p','-movflags','+faststart','-an')

# Cyber Sunset Grid (grid scrolls one cell every 2 s -> loops exactly in 12 s, no crossfade needed)
$fc = @(
  "gradients=s=1920x1080:r=30:d=12:n=3:c0=0x0d0026:c1=0x7a1a8c:c2=0xff6a3d:x0=960:y0=0:x1=960:y1=560:speed=0[bg]",
  "color=c=black:s=600x600:r=30:d=12,format=rgba,geq=r='255':g='230-Y*0.32':b='90+Y*0.1':a='if(lte(hypot(X-300\,Y-300)\,300)\,if(gt(Y\,320)*lt(mod(Y\,44)\,16)\,0\,255)\,0)'[sun]",
  # Analytic perspective grid: u = world x, v = depth (+ scroll 0.5/s -> period 2 s); line width kept ~1.5 px.
  "color=c=black:s=1920x540:r=30:d=12,format=gbrp,geq=g='(10+235*min(1\,Y/140)*max(lt(abs((X-960)*3/(Y+20)-round((X-960)*3/(Y+20)))\,4.5/(Y+20))\,lt(abs(300/(Y+20)+T*0.5-round(300/(Y+20)+T*0.5))\,450/((Y+20)*(Y+20)))))*0.17':b='(20+235*min(1\,Y/140)*max(lt(abs((X-960)*3/(Y+20)-round((X-960)*3/(Y+20)))\,4.5/(Y+20))\,lt(abs(300/(Y+20)+T*0.5-round(300/(Y+20)+T*0.5))\,450/((Y+20)*(Y+20)))))*0.85':r='(10+245*min(1\,Y/140)*max(lt(abs((X-960)*3/(Y+20)-round((X-960)*3/(Y+20)))\,4.5/(Y+20))\,lt(abs(300/(Y+20)+T*0.5-round(300/(Y+20)+T*0.5))\,450/((Y+20)*(Y+20)))))'[floor]",
  "[floor]split[f1][f2];[f2]gblur=sigma=6[fg];[f1][fg]blend=all_mode=screen,format=rgba[floorglow]",
  "[bg][sun]overlay=660:120[s1]",
  "[s1][floorglow]overlay=0:540,drawbox=x=0:y=538:w=1920:h=4:color=0xff6ae8@0.9:t=fill,noise=alls=2:allf=t[v]"
) -join ';'
ffmpeg -y -hide_banner -loglevel error -filter_complex $fc -map "[v]" -t 12 @enc "$Out\cyber_sunset_grid.mp4"
Get-ChildItem $Out | Select-Object Name, Length
