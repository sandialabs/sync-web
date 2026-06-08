;; Imported from upstream s7test.scm line 25097.
;; Original form:
;; (test (object->string (let ((c 3)) (define (ex1) (+ c 1))) :readable) "(let ((c 3)) (lambda () (+ c 1)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((c 3)) (define (ex1) (+ c 1))) :readable))))
       (expected (upstream-safe (lambda () "(let ((c 3)) (lambda () (+ c 1)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25097 actual expected ok?))
