(define (transpose-event event semitones)
  (let ((pitch (cadr (assoc 'pitch event))))
    (map (lambda (pair)
           (if (eq? (car pair) 'pitch)
               (list 'pitch (+ pitch semitones))
               pair))
         event)))

(define phrase
  '(((time 0.0) (dur 0.25) (pitch 60) (amp 0.5))
    ((time 0.25) (dur 0.25) (pitch 64) (amp 0.4))
    ((time 0.5) (dur 0.5) (pitch 67) (amp 0.3))))

(map (lambda (event) (transpose-event event 12)) phrase)
