(let ((sum (do ((i 0 (+ i 1))
                (acc 0 (+ acc i)))
               ((> i 5) acc))))
  (list
    (cond ((< sum 10) 'small)
          ((= sum 15) 'fifteen)
          (else 'other))
    (case sum
      ((0 1 2) 'tiny)
      ((15) 'matched)
      (else 'missed))
    sum))
