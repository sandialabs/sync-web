(lambda (assertions-src standard-src chain-src)

  (eval assertions-src)

  (define standard-class ((eval standard-src) 'class))
  (define load-node (eval '(lambda (node) (sync-eval node))))
  (define combine-node
    (eval '(lambda (trusted node)
             (sync-eval (sync-cons (sync-car trusted) (sync-cdr node))))))
  (define (compile class)
    (let ((method (caddr standard-class)))
      ((eval `(lambda* ,(cddadr method) ,@(cddr method))) class)))
  (define portable (load-node (compile standard-class)))
  (define* (standard (operation #f))
    (case operation
      ((make) compile)
      ((init) (lambda (class . args)
                (let ((object (load-node (compile class))))
                  (if (member '*init* (object '*api*))
                      (apply (object '*init*) args))
                  (object))))
      (else (if operation (portable operation) (portable)))))

  (define (caught thunk)
    (catch #t thunk (lambda args (list 'error (car args)))))

  (define chain-1 (sync-eval (assert ((standard 'init) chain-src) sync-node?)))

  (assert ((chain-1 'size)) 0)

  (assert ((chain-1 'push!) (expression->byte-vector "hello")) #t)
  (assert ((chain-1 'push!) (expression->byte-vector ",")) #t)
  (assert ((chain-1 'push!) (expression->byte-vector "world")) #t)
  (assert ((chain-1 'push!) (expression->byte-vector "!")) #t)

  (assert (list (byte-vector->expression ((chain-1 'get) 0))
                (byte-vector->expression ((chain-1 'get) 1))
                (byte-vector->expression ((chain-1 'get) 2))
                (byte-vector->expression ((chain-1 'get) 3)))
          '("hello" "," "world" "!"))

  (let ((chain (sync-eval (chain-1))))
    (assert ((chain 'set!) 1 (expression->byte-vector ":")) #t)
    (assert (byte-vector->expression ((chain 'get) 1)) ":"))

  (let ((chain (sync-eval (chain-1))))
    (assert ((chain 'slice!) 1) #t)
    (assert (byte-vector->expression ((chain 'get) 1)) ","))

  (let ((chain (sync-eval (chain-1))))
    (assert ((chain 'prune!) 1) #t)
    (assert ((chain 'get) 1) '(unknown))
    (assert (byte-vector->expression ((chain 'get) -1)) "!"))

  (let ((chain (sync-eval (chain-1))))
    (let ((digest (sync-digest (chain))))
      (assert ((chain 'truncate!) 1) (lambda (x) (sync-node? x)))
      (assert (equal? digest (sync-digest (chain))) #t)
      (assert ((chain 'get) 0) '(unknown))
      (assert ((chain 'get) 1) '(unknown))
      (assert (byte-vector->expression ((chain 'get) 2)) "world")
      (assert (byte-vector->expression ((chain 'get) 3)) "!")
      (assert ((chain 'push!) (expression->byte-vector "next")) #t)
      (assert (byte-vector->expression ((chain 'get) -1)) "next")))

  ;; Truncation uses the same inclusive index contract in both chain classes.
  (let ((chain (sync-eval (chain-1))))
    (assert (caught (lambda () ((chain 'truncate!) -5)))
            (lambda (result) (equal? result '(error index-error))))
    (assert (caught (lambda () ((chain 'truncate!) 4)))
            (lambda (result) (equal? result '(error index-error)))))

  (let ((chain-1 (sync-eval ((standard 'init) chain-src)))
        (chain-2 (sync-eval ((standard 'init) chain-src)))
        (chain-3 (sync-eval ((standard 'init) chain-src))))
    ((chain-1 'push!) (expression->byte-vector "hello"))
    ((chain-1 'push!) (expression->byte-vector ","))
    ((chain-1 'push!) (expression->byte-vector "world"))
    ((chain-1 'push!) (expression->byte-vector "!"))
    ((chain-2 'push!) (expression->byte-vector "hello"))
    ((chain-2 'push!) (expression->byte-vector ","))
    ((chain-2 'push!) (expression->byte-vector "world"))
    ((chain-2 'push!) (expression->byte-vector "!"))
    ((chain-3 'push!) (expression->byte-vector "hello"))
    ((chain-3 'push!) (expression->byte-vector ","))
    ((chain-3 'push!) (expression->byte-vector "world"))
    ((chain-3 'push!) (expression->byte-vector "!"))
    ((chain-2 'push!) (expression->byte-vector "this"))
    ((chain-2 'push!) (expression->byte-vector "is"))
    ((chain-3 'push!) (expression->byte-vector "this"))
    ((chain-3 'push!) (expression->byte-vector "is"))
    ((chain-3 'push!) (expression->byte-vector "a"))
    ((chain-3 'push!) (expression->byte-vector "somewhat"))
    ((chain-3 'push!) (expression->byte-vector "longer"))
    ((chain-3 'push!) (expression->byte-vector "chain"))
    (assert (and (equal? ((chain-1 'digest)) ((chain-2 'digest) -3))
                 (equal? ((chain-1 'digest)) ((chain-3 'digest) -7)))
            #t))

  (let ((chain (sync-eval ((standard 'init) chain-src))))
    (let loop ((i 100))
      (if (= i 0)
          (assert ((chain 'size)) 100)
          (begin
            (assert ((chain 'push!) #u()) #t)
            (loop (- i 1))))))

  ;; Increasing truncation indexes monotonically hide the exact inclusive
  ;; prefix, preserve every newer historical digest, and remain append-compatible.
  (let* ((size 17)
         (new-chain
          (lambda ()
            (let ((chain (sync-eval ((standard 'init) chain-src))))
              (let loop ((i 0))
                (if (= i size) chain
                    (begin
                      ((chain 'push!) (expression->byte-vector i))
                      (loop (+ i 1))))))))
         (reference (new-chain)))
    ((reference 'push!) (expression->byte-vector 'next))
    (let ((expected (sync-digest (reference))))
      (for-each
       (lambda (index)
         (let ((chain (new-chain)))
           (let ((before (sync-digest (chain))))
             ((chain 'truncate!) index)
             (assert
              (and (equal? before (sync-digest (chain)))
                   (let loop ((i 0))
                     (or (= i size)
                         (and (if (<= i index)
                                  (equal? ((chain 'get) i) '(unknown))
                                  (= (byte-vector->expression ((chain 'get) i)) i))
                              (loop (+ i 1)))))
                   (let loop ((i (+ index 1)))
                     (or (= i size)
                         (and (equal? (sync-digest ((reference 'previous) i))
                                      (sync-digest ((chain 'previous) i)))
                              (loop (+ i 1)))))
                   (eq? ((chain 'push!) (expression->byte-vector 'next)) #t)
                   (equal? expected (sync-digest (chain)))
                   (equal? (byte-vector->expression ((chain 'get) -1)) 'next))
              #t))))
       '(0 1 2 3 7 8 15 16))))

  ;; Truncating a fixed retained suffix keeps only logarithmic chain structure.
  ;; Count the proof tree rather than class code/state headers.
  (let ((node-count
         (lambda (root)
           (let count ((node root))
             (if (sync-pair? node)
                 (+ 1 (count (sync-car node)) (count (sync-cdr node)))
                 1))))
        (bits (lambda (x)
                (let loop ((x x) (count 0))
                  (if (= x 0) count (loop (ash x -1) (+ count 1))))))
        (window 8))
    (for-each
     (lambda (size)
       (let ((chain (sync-eval ((standard 'init) chain-src))))
         (let fill ((i 0))
           (if (< i size)
               (begin ((chain 'push!) (expression->byte-vector i))
                      (fill (+ i 1)))))
         ((chain 'truncate!) (- size window 1))
         (assert (<= (node-count (chain '(1 1)))
                     (+ (* 2 window) (* 2 (bits size)) 4))
                 #t)))
     '(64 256 1024)))

  ;; A larger window shrink followed by the next rolling truncation remains
  ;; composable over the canonical frontier created by the earlier window.
  (if (eq? (chain-1 '*name*) 'log-chain)
      (let ((chain (sync-eval ((standard 'init) chain-src)))
            (reference (sync-eval ((standard 'init) chain-src))))
    (let fill ((i 0))
      (if (< i 257)
          (begin
            ((chain 'push!) (expression->byte-vector i))
            ((reference 'push!) (expression->byte-vector i))
            (if (> ((chain 'size)) 16)
                ((chain 'truncate!) (- ((chain 'size)) 17)))
            (fill (+ i 1)))))
    ((chain 'truncate!) 252)
    ((chain 'push!) (expression->byte-vector 257))
    ((reference 'push!) (expression->byte-vector 257))
    (assert ((chain 'truncate!) 253) (lambda (node) (sync-node? node)))
    (assert ((chain 'get) 253) '(unknown))
    (assert (byte-vector->expression ((chain 'get) 254)) 254)
    (assert (byte-vector->expression ((chain 'get) 257)) 257)
    (if (eq? (chain '*name*) 'log-chain)
        (for-each
         (lambda (index)
           (assert (equal? (sync-digest ((chain 'previous) index))
                           (sync-digest ((reference 'previous) index))) #t))
         '(254 255 256 257)))))

  ;; Widening is non-resurrecting: a lower requested cutoff keeps existing
  ;; stumps while future appends and rolling truncation remain valid.
  (if (eq? (chain-1 '*name*) 'log-chain)
      (let ((chain (sync-eval ((standard 'init) chain-src))))
        (let fill ((i 0))
          (if (< i 64)
          (begin ((chain 'push!) (expression->byte-vector i))
                 (fill (+ i 1)))))
    ((chain 'truncate!) 55)
    ((chain 'truncate!) 47)
    (assert ((chain 'get) 55) '(unknown))
    ((chain 'push!) (expression->byte-vector 64))
    (assert ((chain 'truncate!) 56) (lambda (node) (sync-node? node)))
        (assert ((chain 'get) 56) '(unknown))
        (assert (byte-vector->expression ((chain 'get) 64)) 64)))

  ;; Pruning entries hides their values without preventing derivation of later
  ;; historical prefixes. Linear and log-structured chains share this public
  ;; proof contract even though their internal layouts differ.
  (let* ((size 16)
         (cutoff 7)
         (new-chain
          (lambda ()
            (let ((chain (sync-eval ((standard 'init) chain-src))))
              (let loop ((i 0))
                (if (= i size) chain
                    (begin
                      ((chain 'push!) (expression->byte-vector i))
                      (loop (+ i 1))))))))
         (reference (new-chain))
         (chain (new-chain)))
    (let prune ((i 0))
      (if (<= i cutoff)
          (begin ((chain 'prune!) i) (prune (+ i 1)))))
    (for-each
     (lambda (selected)
       (let ((prefix (sync-eval ((chain 'previous) selected))))
         (assert (equal? (sync-digest ((reference 'previous) selected))
                         (sync-digest (prefix)))
                 #t)
         (let check ((i 0))
           (if (<= i selected)
               (begin
                 (assert (if (<= i cutoff)
                             (equal? ((prefix 'get) i) '(unknown))
                             (= (byte-vector->expression ((prefix 'get) i)) i))
                         #t)
                 (check (+ i 1)))))))
     '(5 9)))

  ;; Appending to a sliced proof either preserves the supplied latest value or
  ;; deliberately rejects unavailable structure before mutation.
  (let* ((size 16)
         (new-chain
          (lambda ()
            (let ((chain (sync-eval ((standard 'init) chain-src))))
              (let loop ((i 0))
                (if (= i size) chain
                    (begin
                      ((chain 'push!) (expression->byte-vector i))
                      (loop (+ i 1))))))))
         (reference (new-chain)))
    ((reference 'push!) (expression->byte-vector 'next))
    (let ((expected (sync-digest (reference))))
      (for-each
       (lambda (index)
         (let ((chain (new-chain)))
           ((chain 'slice!) index)
           (let ((before (sync-digest (chain)))
                 (result (caught
                          (lambda ()
                            ((chain 'push!) (expression->byte-vector 'next))))))
             (assert
              (if (equal? result '(error availability-error))
                  (and (= ((chain 'size)) size)
                       (equal? before (sync-digest (chain))))
                  (and (eq? result #t)
                       (= ((chain 'size)) (+ size 1))
                       (equal? expected (sync-digest (chain)))
                       (equal? (byte-vector->expression ((chain 'get) -1))
                               'next)))
              #t))))
       '(0 11 14 15))))

  (let* ((size 100)
         (chain (sync-eval ((standard 'init) chain-src)))
         (digests-1 (let loop ((i 0) (digests '()))
                      (if (= i size) (reverse digests)
                          (begin
                            ((chain 'push!) (sync-null))
                            (loop (+ i 1) (cons ((chain 'digest)) digests))))))
         (digests-2 (let loop ((i 0) (digests '()))
                      (if (= i size) (reverse digests)
                          (loop (+ i 1) (cons ((chain 'digest) i) digests))))))
    (assert (equal? digests-1 digests-2) #t))

  (append "Success (" (object->string asserted) " checks)"))
