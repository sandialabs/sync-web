use hex;
use journal_sdk::{JOURNAL, SIZE, Word};
use rand::RngCore;

fn fresh_record_hex() -> String {
    let mut seed: Word = [0; SIZE];
    rand::thread_rng().fill_bytes(&mut seed);
    hex::encode(seed)
}

fn setup_record() -> (String, impl Fn(&str, &str)) {
    let record = fresh_record_hex();

    assert_eq!(
        JOURNAL.evaluate(
            format!("(sync-create (hex-string->byte-vector \"{}\"))", record).as_str(),
        ),
        "#t",
        "Unable to set up new Journal record",
    );

    (record.clone(), move |expression, expected| {
        let result = JOURNAL.evaluate(
            format!(
                "(sync-call '{} #t (hex-string->byte-vector \"{}\"))",
                expression, record,
            )
            .as_str(),
        );
        assert_eq!(result, expected, "Assertion failed: {}", expression);
    })
}

#[test]
fn test_sync_hash_and_digest_on_byte_vectors() {
    assert_eq!(
        JOURNAL.evaluate("(equal? (sync-digest #u(1 2 3)) (sync-hash #u(1 2 3)))"),
        "#t",
    );
}

#[test]
fn test_sync_predicates_on_byte_vectors() {
    assert_eq!(JOURNAL.evaluate("(sync-node? #u(1 2 3))"), "#f");
    assert_eq!(JOURNAL.evaluate("(sync-null? #u(1 2 3))"), "#f");
    assert_eq!(JOURNAL.evaluate("(sync-pair? #u(1 2 3))"), "#f");
    assert_eq!(JOURNAL.evaluate("(sync-stub? #u(1 2 3))"), "#f");
}

#[test]
fn test_sync_node_structure_primitives() {
    assert_eq!(JOURNAL.evaluate("(sync-null? (sync-null))"), "#t");
    assert_eq!(JOURNAL.evaluate("(sync-pair? (sync-cons (sync-null) (sync-null)))"), "#t");
    assert_eq!(
        JOURNAL.evaluate("(sync-null? (sync-car (sync-cons (sync-null) #u(1 2 3))))"),
        "#t",
    );
    assert_eq!(
        JOURNAL.evaluate("(equal? (sync-cdr (sync-cons (sync-null) #u(1 2 3))) #u(1 2 3))"),
        "#t",
    );
    assert_eq!(
        JOURNAL.evaluate("(sync-stub? (sync-cut (sync-cons (sync-null) #u(1 2 3))))"),
        "#t",
    );
    assert_eq!(
        JOURNAL.evaluate("(sync-stub? (sync-stub (sync-hash #u(1 2 3))))"),
        "#t",
    );
}

#[test]
fn test_sync_serialization_primitives() {
    let (_record, assert) = setup_record();
    assert(
        "(let* ((root (sync-cons #u(1) #u(2)))\
                (serialization (sync-serialize root #f))\
                (copy (sync-deserialize (reverse serialization))))\
           (and (equal? serialization \
                        '((n-1 (p n-2 n-3))\
                          (n-3 (c #u(2)))\
                          (n-2 (c #u(1)))))\
                (equal? (sync-digest root) (sync-digest copy))))",
        "#t",
    );
    assert(
        "(let* ((root (sync-cons #u(1) #u(2)))\
                (serialization\
                 (sync-serialize root '(lambda (node) (sync-car node))))\
                (copy (sync-deserialize serialization)))\
           (and (equal? serialization \
                        `((n-1 (p n-2 n-3))\
                          (n-3 (s ,(sync-digest (sync-cut #u(2)))))\
                          (n-2 (c #u(1)))))\
                (equal? (sync-digest root) (sync-digest copy))))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda () (sync-deserialize '((n-1 (p missing n-0)))) #f)\
           (lambda args (eq? (car args) 'serialization-error)))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda () (sync-deserialize '((n-1 (s #u(1 2 3))))) #f)\
           (lambda args (eq? (car args) 'wrong-type-arg)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize \
                root \
                '(lambda (node)\
                   (sync-serialize node '(lambda (inner) (sync-car inner))))))\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize root '(lambda (node) (sync-state)))\
               #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize\
                root\
                '(lambda (node)\
                   (set! (setter list) (lambda (value) value))))\
               #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (list?\
            (sync-serialize \
             root \
             '(lambda (node)\
                (if (setter car) (error 'setter-escape) #t)))))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize \
                root \
                '(lambda (node)\
                   (let ((local (lambda () #t)))\
                     (set! (setter local) car))))\
               #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize root '(lambda (node) (rootlet)))\
               #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize root '(lambda (node) (sync-call '(+ 1 2) #t)))\
               #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize root '(lambda (node) (error 'expected \"safe\")))\
               #f)\
             (lambda args (eq? (car args) 'expected))))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize \
                root \
                '(begin\
                   (error 'compile-expected \"safe\")\
                   (lambda (node) node)))\
               #f)\
             (lambda args (eq? (car args) 'compile-expected))))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize \
                root \
                '(begin\
                   (error 'escape (lambda () 'secret))\
                   (lambda (node) node)))\
               #f)\
             (lambda args (eq? (car args) 'serialization-error))))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize \
                root \
                '(lambda (node) (error 'escape (lambda (value) value))))\
               #f)\
             (lambda args (eq? (car args) 'serialization-error))))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize root '(lambda (node) (error 'expected \"stop\"))))\
             (lambda args #t))\
           (equal? (sync-serialize root '(lambda (node) (sync-car node)))\
                   `((n-1 (p n-2 n-3))\
                     (n-3 (s ,(sync-digest (sync-cut #u(2)))))\
                     (n-2 (c #u(1))))))",
        "#t",
    );
    assert(
        "(let* ((root (sync-cons #u(1) #u(2)))\
                (query '(lambda (node) (%sync-let-environment 'counter)))\
                (run (lambda ()\
                       (catch #t\
                         (lambda () (sync-serialize root query) #f)\
                         (lambda args (car args))))))\
           (let ((first (run)) (second (run)))\
             (and first second (eq? first second))))",
        "#t",
    );
    assert(
        "(let ((root (sync-cons #u(1) #u(2))))\
           (catch #t\
             (lambda ()\
               (sync-serialize root '(lambda (node) (set! + 1)))\
               #f)\
             (lambda args #t))\
           (= (+ 1 2) 3))",
        "#t",
    );
    assert(
        "(let* ((root (sync-cons #u(1) #u(2)))\
                (query \
                 '(let ((count 0))\
                    (lambda (node)\
                      (set! count (+ count 1))\
                      (if (= count 1) (sync-car node) (sync-cdr node)))))\
                (first (sync-serialize root query))\
                (second (sync-serialize root query)))\
           (equal? first second))",
        "#t",
    );
    assert(
        "(let* ((root \
                 (let loop ((i 5000) (node (sync-null)))\
                   (if (= i 0) node\
                       (loop (- i 1)\
                             (sync-cons (expression->byte-vector i) node)))))\
                (serialization (sync-serialize root #f))\
                (copy (sync-deserialize serialization)))\
           (and (= (length serialization) 10000)\
                (equal? (sync-digest root) (sync-digest copy))))",
        "#t",
    );
}


#[test]
fn test_sync_state_and_sync_eval() {
    let (_record, assert) = setup_record();
    assert("(sync-node? (sync-state))", "#t");
    assert(
        "(let* ((code '(lambda (state) (define* (self (arg #f)) (if arg arg state))))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           ((sync-eval node) 'hello))",
        "hello",
    );
    assert(
        "(let* ((code '(lambda (state) (lambda () current-value)))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           (let ((current-value 'local)) ((sync-eval node))))",
        "local",
    );
    assert(
        "(let* ((code '(lambda (state) (lambda () (+ 1 2))))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           (let ((+ (lambda args 'local-plus))) ((sync-eval node))))",
        "local-plus",
    );
    assert(
        "(let* ((code '(lambda (state) (lambda () (local-macro value))))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           (let ((local-macro (macro (value) `',value)))\
             ((sync-eval node))))",
        "value",
    );
    assert(
        "(let* ((code '(lambda (state)\
                         (lambda () `(literal ,(if (sync-node? state) 'node 'bad)))))\
                (node (sync-cons (expression->byte-vector code) #u(9))))\
           ((sync-eval node)))",
        "(literal node)",
    );
    assert(
        "(let* ((code '(lambda (state) (lambda () state)))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           (catch #t (lambda () (sync-eval node #f) #f) (lambda args #t)))",
        "#t",
    );
}

#[test]
fn test_sync_let() {
    let (_record, assert) = setup_record();
    assert("(sync-let ((x 2) (y 3)) (+ x y))", "5");
    assert(
        "(let* ((code '(lambda (node) (lambda (value) (+ 3 value))))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           (sync-let ((node node) (value 4)) ((sync-eval node) value)))",
        "7",
    );
    assert(
        "(let* ((code '(lambda (node) (lambda () (rootlet))))\
                (node (sync-cons (expression->byte-vector code) (sync-null))))\
           (catch #t\
             (lambda () (sync-let ((node node)) ((sync-eval node))) #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((node (sync-cons #u(1) #u(2))))\
           (sync-node? (car (sync-let ((items (list node))) items))))",
        "#t",
    );
    assert(
        "(let ((host-vector (vector 1)))\
           (sync-let ((sandbox-vector host-vector))\
             (set! (sandbox-vector 0) 2)\
             sandbox-vector)\
           (= (host-vector 0) 1))",
        "#t",
    );
    assert(
        "(let ((host-secret \"secret\"))\
           (catch #t (lambda () (sync-let () host-secret) #f) (lambda args #t)))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (sync-state)) #f) (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (eval '(+ 1 2))) #f) (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda ()\
             (sync-let () (define-macro (m) '(+ 1 2)) (m)) #f)\
           (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda ()\
             (sync-let () (define-bacro (m) '(+ 1 2)) (m)) #f)\
           (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (sync-call '(+ 1 2) #t)) #f)\
             (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (format #t \"forbidden\")) #f)\
             (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (((rootlet) 'sync-state))) #f)\
             (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (list (lambda (x) x))) #f)\
             (lambda args (eq? (car args) 'sync-web-error)))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda () (sync-let () (error 'escape (lambda (x) x))) #f)\
           (lambda args (eq? (car args) 'sync-web-error)))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (error 'sandbox-error \"safe\")) #f)\
             (lambda args (and (eq? (car args) 'sandbox-error)\
                               (equal? (cadr args) '(\"safe\")))))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let () (values 1 2)) #f) (lambda args #t))",
        "#t",
    );
    assert(
        "(begin (sync-let () (set! + 1) #t) (= (+ 1 2) 3))",
        "#t",
    );
    assert(
        "(and (not (sync-let-active?))\
              (sync-let () (sync-let-active?)))",
        "#t",
    );
    assert(
        "(sync-let ()\
           (letrec ((value 0)\
                    (self (lambda () value))\
                    (set (lambda (next) (set! value next))))\
             (set! (setter self) set)\
             (set! (self) 7)\
             (self)))",
        "7",
    );
    assert(
        "(catch #t\
           (lambda () (sync-let-eval '(+ 1 2)) #f)\
           (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda () (%sync-safe-setter-set! list (lambda (value) value)) #f)\
           (lambda args #t))",
        "#t",
    );
    assert(
        "(let ((secret 9))\
           (sync-let ((expr '(lambda () secret)))\
             (catch #t\
               (lambda ()\
                 (if (equal? ((sync-let-eval expr)) 9) 'leaked 'isolated))\
               (lambda args 'isolated))))",
        "isolated",
    );
    assert(
        "(catch #t\
           (lambda () (%sync-let-return (%sync-let '(x) '() '(begin x))) #f)\
           (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda () (%sync-let-return (%sync-let '(x x) '(1 2) '(begin x))) #f)\
           (lambda args #t))",
        "#t",
    );
    assert(
        "(catch #t (lambda () (sync-let ((if 7)) if) #f) (lambda args #t))",
        "#t",
    );
    assert(
        "(let* ((host-secret \"escaped\") (f (lambda () host-secret)))\
           (eq? (car (%sync-let '() '() (list 'begin (list f))))\
                '%sync-let-error))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda ()\
             (sync-let () (set! list 1) (error 'expected \"failure\")) #f)\
           (lambda args (eq? (car args) 'expected)))",
        "#t",
    );
    assert(
        "(catch #t\
           (lambda ()\
             (sync-let ()\
               (set! list (lambda args (cons '%sync-let-ok (cons 'forged '()))))\
               (error 'expected \"failure\"))\
             #f)\
           (lambda args (eq? (car args) 'expected)))",
        "#t",
    );
    assert(
        "(let ((cycle (list 1)))\
           (set-cdr! cycle cycle)\
           (catch #t (lambda () (sync-let ((cycle cycle)) cycle) #f)\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(do ((i 0 (+ i 1)))\
             ((= i 1000) #t)\
           (catch #t (lambda () (sync-let () (error 'expected \"failure\")))\
             (lambda args #t)))",
        "#t",
    );
    assert(
        "(let ((nested 1))\
           (do ((i 0 (+ i 1))) ((= i 10000)) (set! nested (vector nested)))\
           (sync-let ((nested nested)) #t))",
        "#t",
    );
    assert(
        "(begin\
           (sync-let () #t)\
           (let ((before (length (symbol-table))))\
             (do ((i 0 (+ i 1))) ((= i 2000)) (sync-let () #t))\
             (= before (length (symbol-table)))))",
        "#t",
    );
}

#[test]
fn test_sync_create_all_and_delete() {
    let record = fresh_record_hex();

    assert_eq!(
        JOURNAL.evaluate(
            format!("(sync-create (hex-string->byte-vector \"{}\"))", record).as_str(),
        ),
        "#t",
    );

    assert_eq!(
        JOURNAL.evaluate(
            format!(
                "(not (not (member (hex-string->byte-vector \"{}\") (sync-all))))",
                record
            )
            .as_str(),
        ),
        "#t",
    );

    assert_eq!(
        JOURNAL.evaluate(
            format!("(sync-delete (hex-string->byte-vector \"{}\"))", record).as_str(),
        ),
        "#t",
    );

    assert_eq!(
        JOURNAL.evaluate(
            format!(
                "(not (not (member (hex-string->byte-vector \"{}\") (sync-all))))",
                record
            )
            .as_str(),
        ),
        "#f",
    );
}

#[test]
fn test_sync_call() {
    let (_record, assert) = setup_record();
    assert("(sync-call '(+ 2 2) #t)", "4");
}
