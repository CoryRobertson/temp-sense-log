import time
import board
import adafruit_sht4x
import adafruit_ahtx0
import busio
import math
from math import atan2, degrees

i2c = busio.I2C(board.GP1, board.GP0)
sensor = adafruit_ahtx0.AHTx0(i2c)

while 1:
    temp = sensor.temperature
    humid = sensor.relative_humidity
    t = str("T:%0.1f:" % temp)
    h = str("H:%0.1f:" % humid)
    print(t + h)
    time.sleep(1)
