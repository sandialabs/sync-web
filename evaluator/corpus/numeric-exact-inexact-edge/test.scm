(list
  (list 'integer-rational-real (integer? 1) (rational? 1/2) (real? 1.0) (complex? 1+i))
  (list 'mixed-arithmetic (+ 1 1/2) (+ 1/2 0.25) (* 3+4i 2))
  (list 'rounding (floor -2.3) (ceiling -2.3) (truncate -2.7) (round 2.5))
  (list 'magnitude (abs -5) (magnitude 3+4i) (angle 1+i))
  (list 'complex (real-part 3+4i) (imag-part 3+4i) (- (imag-part 3+4i))))
