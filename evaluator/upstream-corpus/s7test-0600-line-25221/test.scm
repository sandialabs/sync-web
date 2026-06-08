;; Imported from upstream s7test.scm line 25221.
;; Original form:
;; (test (object->string (inlet 'a (let ((b 1)) (lambda () (+ b c)))) :readable) "(inlet :a (let ((b 1)) (lambda () (+ b c))))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (inlet 'a (let ((b 1)) (lambda () (+ b c)))) :readable))))
       (expected (upstream-safe (lambda () "(inlet :a (let ((b 1)) (lambda () (+ b c))))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25221 actual expected ok?))
